use super::model::*;
use super::ownership::resolve_owning_project;
use super::store::ProgressStore;

/// Is Progress collecting for this agent?
///
/// One rule, one place: `global AND (per_agent ?? true)`. The same answer gates
/// the MCP tool listing, the `initialize` obligation, the reporting tool and
/// `intent:` capture — a listed tool with no stated obligation, or an
/// obligation naming a tool that is not listed, are the two ways this drifts.
///
/// The global half is read from the live `AppState`, not from disk: a toggle
/// flipped in Settings must take effect on the next report, and `config.json`
/// is only the persistence of that value.
pub fn progress_tracking_enabled(state: &crate::state::AppState, agent_type: Option<&str>) -> bool {
    progress_tracking_enabled_in(
        state.config.read().progress_tracking,
        agent_type,
        &crate::config::load_agents_config(),
    )
}

pub(crate) fn progress_tracking_enabled_in(
    global: bool,
    agent_type: Option<&str>,
    agents: &crate::config::AgentsConfig,
) -> bool {
    global
        && agent_type
            .and_then(|agent| agents.agents.get(agent))
            .and_then(|settings| settings.progress_tracking)
            .unwrap_or(true)
}

/// The error a caller gets when it reports while collection is off. It names
/// the setting, because the agent cannot change it and the user can.
pub(crate) const TRACKING_DISABLED: &str =
    "progress_tracking_disabled: Progress collection is off for this agent (Settings → Progress)";

/// The one write path. Both halves of the journal — what an agent reports and
/// what TUIC observes — pass the same gate, the same validation and the same
/// ownership resolution, in that order. Which kind is allowed in is decided
/// before this, by the caller that built the entry.
fn append(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    entry: NewProgressEntry,
    agent_type: Option<&str>,
) -> Result<ProgressEntry, String> {
    if !progress_tracking_enabled(state, agent_type) {
        return Err(TRACKING_DISABLED.to_string());
    }
    entry.validate()?;
    let project = resolve_owning_project(project_hint)?;
    ProgressStore::open()?.record(&project.to_string_lossy(), &entry)
}

/// Shared reporting core for MCP, HTTP and Tauri IPC. `into_entry` is what
/// refuses `intent` here: an agent may not claim one.
pub fn submit_progress_report(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    input: ProgressReportInput,
    agent_name: Option<String>,
    agent_type: Option<&str>,
) -> Result<ProgressEntry, String> {
    append(
        state,
        project_hint,
        input.into_entry(agent_name)?,
        agent_type,
    )
}

/// Record an `intent:` marker the agent already emitted.
///
/// This is the host's half of the journal and the reliability floor: the
/// reporting obligation sits hours back in an `initialize` blob, while this
/// trigger fires on every task.
pub fn record_intent(
    state: &crate::state::AppState,
    project_hint: Option<&str>,
    text: &str,
    agent_name: Option<String>,
    agent_type: Option<&str>,
) -> Result<ProgressEntry, String> {
    append(
        state,
        project_hint,
        NewProgressEntry {
            kind: ProgressKind::Intent,
            text: text.to_string(),
            step: None,
            agent_name,
        },
        agent_type,
    )
}

fn project_of(project: &str) -> Result<String, String> {
    Ok(resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string())
}

pub fn progress_list(project: &str, input: ProgressListInput) -> Result<ProgressList, String> {
    ProgressStore::open()?.list(&project_of(project)?, &input)
}

pub fn progress_delete(
    project: &str,
    input: ProgressDeleteInput,
) -> Result<ProgressDeleteReceipt, String> {
    ProgressStore::open()?.delete(&project_of(project)?, &input.ids)
}

pub fn progress_mark_viewed(project: &str) -> Result<ProgressViewedReceipt, String> {
    ProgressStore::open()?.mark_viewed(&project_of(project)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentSettings, AgentsConfig};

    fn agents_with(agent: &str, progress_tracking: Option<bool>) -> AgentsConfig {
        let mut agents = AgentsConfig::default();
        agents.agents.insert(
            agent.to_string(),
            AgentSettings {
                progress_tracking,
                ..Default::default()
            },
        );
        agents
    }

    /// `global AND (per_agent ?? true)` — all four corners, plus the two ways
    /// an agent can be silent about it (no entry, or an entry with no opinion).
    #[test]
    fn the_effective_flag_is_global_and_the_agent_override() {
        let opted_out = agents_with("claude", Some(false));
        let opted_in = agents_with("claude", Some(true));
        let no_opinion = agents_with("claude", None);

        assert!(progress_tracking_enabled_in(
            true,
            Some("claude"),
            &opted_in
        ));
        assert!(!progress_tracking_enabled_in(
            true,
            Some("claude"),
            &opted_out
        ));
        assert!(!progress_tracking_enabled_in(
            false,
            Some("claude"),
            &opted_in
        ));
        assert!(!progress_tracking_enabled_in(
            false,
            Some("claude"),
            &opted_out
        ));

        // A missing override means yes; so does an unknown agent and a caller
        // that is not an agent at all (the desktop UI, a local HTTP client).
        assert!(progress_tracking_enabled_in(
            true,
            Some("claude"),
            &no_opinion
        ));
        assert!(progress_tracking_enabled_in(
            true,
            Some("codex"),
            &opted_out
        ));
        assert!(progress_tracking_enabled_in(true, None, &opted_out));
    }

    /// The host writes `intent`, so it is the one kind the journal accepts from
    /// nobody else — and the entry still carries which agent said it.
    #[test]
    fn an_intent_marker_is_recorded_against_the_project_and_the_agent() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();

        let entry = record_intent(
            &state,
            Some(&project.path().to_string_lossy()),
            "Rewriting the Progress store",
            Some("claude".to_string()),
            Some("claude"),
        )
        .unwrap();
        assert_eq!(entry.kind, ProgressKind::Intent);
        assert_eq!(entry.text, "Rewriting the Progress store");
        assert_eq!(entry.agent_name.as_deref(), Some("claude"));
        assert_eq!(
            entry.project,
            project.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    /// Collection off means the journal does not grow — not that it grows more
    /// quietly. Both the reporting path and the host's own capture stop here.
    #[test]
    fn nothing_is_written_while_collection_is_off() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().progress_tracking = false;

        let hint = project.path().to_string_lossy().to_string();
        assert_eq!(
            record_intent(&state, Some(&hint), "Should not land", None, None).unwrap_err(),
            TRACKING_DISABLED
        );
        assert_eq!(
            submit_progress_report(
                &state,
                Some(&hint),
                ProgressReportInput {
                    kind: ProgressKind::Done,
                    text: "Should not land either".to_string(),
                    step: None,
                },
                None,
                None,
            )
            .unwrap_err(),
            TRACKING_DISABLED
        );
        assert!(
            progress_list(&hint, ProgressListInput::default())
                .unwrap()
                .entries
                .is_empty()
        );
    }

    /// A PTY runs wherever the user started it. When that directory belongs to
    /// no registered repository there is no project to attribute an `intent:`
    /// to, and the focused UI repository is emphatically not the answer — it
    /// would file one tab's work under whatever the user happened to be looking
    /// at. The caller resolves the project through `registered_repo_for_path`
    /// and drops the marker when it answers `None`.
    #[test]
    fn an_unregistered_working_directory_resolves_to_no_project() {
        use crate::mcp_http::mcp_transport::registered_repo_for_path;

        let known = vec![
            "/Users/dev/one".to_string(),
            "/Users/dev/one/two".to_string(),
        ];
        assert_eq!(registered_repo_for_path("/tmp/scratch", &known), None);
        assert_eq!(
            registered_repo_for_path("/Users/dev/one/src/lib.rs", &known).as_deref(),
            Some("/Users/dev/one")
        );
        // The deepest registered repository wins, so a nested registration is
        // never filed under its parent.
        assert_eq!(
            registered_repo_for_path("/Users/dev/one/two/src", &known).as_deref(),
            Some("/Users/dev/one/two")
        );
        // A sibling that merely shares a prefix is not inside anything.
        assert_eq!(registered_repo_for_path("/Users/dev/oneiric", &known), None);
    }
}
