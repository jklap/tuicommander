use super::{
    NewPlan, NewStory, Plan, PlanSource, PlanState, PlanView, Story, StoryCommand, StoryStore,
    StoryTransition,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryAction {
    CreatePlan {
        title: String,
        source: String,
    },
    ListPlans,
    ListPlanSources,
    AddPlanSource {
        source: String,
    },
    GetPlan {
        plan_id: String,
    },
    PlanState {
        plan_id: String,
    },
    PlanView {
        plan_id: String,
    },
    CreateStory {
        input: NewStory,
    },
    ListStories {
        plan_id: String,
    },
    GetStory {
        story_id: String,
    },
    TransitionHistory {
        story_id: String,
    },
    AddDependency {
        story_id: String,
        dependency_id: String,
        expected_revision: i64,
    },
    RemoveDependency {
        story_id: String,
        dependency_id: String,
        expected_revision: i64,
    },
    Claim {
        story_id: String,
        expected_revision: i64,
    },
    Transition {
        story_id: String,
        expected_revision: i64,
        command: StoryCommand,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StoryReply {
    Plan(Plan),
    Plans(Vec<Plan>),
    PlanSources(Vec<PlanSource>),
    PlanState(PlanState),
    PlanView(PlanView),
    Story(Story),
    Stories(Vec<Story>),
    Transitions(Vec<StoryTransition>),
}

/// All transports use this boundary, so an identifier alone never grants cross-project access.
pub fn story_action(
    project: &str,
    action: StoryAction,
    actor_session: Option<&str>,
) -> Result<StoryReply, String> {
    story_action_with_source(project, action, actor_session, false)
}

fn story_action_with_source(
    project: &str,
    action: StoryAction,
    actor_session: Option<&str>,
    unauthenticated_http: bool,
) -> Result<StoryReply, String> {
    if !crate::fs::is_absolute_on_any_platform(project) {
        return Err("project must be an absolute path".into());
    }
    let project = crate::progress::resolve_owning_project(Some(project))?
        .to_string_lossy()
        .to_string();
    let store = StoryStore::open()?;
    let plan_in_project = |plan_id: &str| -> Result<Plan, String> {
        let plan = store.get_plan(plan_id)?;
        if plan.project != project {
            return Err("plan does not belong to project".into());
        }
        Ok(plan)
    };
    let story_in_project = |story_id: &str| -> Result<Story, String> {
        let story = store.get_story(story_id)?;
        plan_in_project(&story.plan_id)?;
        Ok(story)
    };
    match action {
        StoryAction::CreatePlan { title, source } => {
            Ok(StoryReply::Plan(store.create_plan(NewPlan {
                project: project.clone(),
                title,
                source,
            })?))
        }
        StoryAction::ListPlans => Ok(StoryReply::Plans(store.list_plans(&project)?)),
        StoryAction::ListPlanSources => Ok(StoryReply::PlanSources(
            super::sources::list_plan_sources(std::path::Path::new(&project))?,
        )),
        StoryAction::AddPlanSource { source } => {
            if let Some(plan) = store
                .list_plans(&project)?
                .into_iter()
                .find(|plan| plan.source == source)
            {
                return Ok(StoryReply::Plan(plan));
            }
            let title = super::sources::title_for_source(std::path::Path::new(&project), &source)?;
            Ok(StoryReply::Plan(store.create_plan(NewPlan {
                project: project.clone(),
                title,
                source,
            })?))
        }
        StoryAction::GetPlan { plan_id } => Ok(StoryReply::Plan(plan_in_project(&plan_id)?)),
        StoryAction::PlanState { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanState(store.plan_state(&plan_id)?))
        }
        StoryAction::PlanView { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanView(store.plan_view(&plan_id)?))
        }
        StoryAction::CreateStory { input } => {
            plan_in_project(&input.plan_id)?;
            Ok(StoryReply::Story(store.create_story(input)?))
        }
        StoryAction::ListStories { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::Stories(store.list_stories(&plan_id)?))
        }
        StoryAction::GetStory { story_id } => Ok(StoryReply::Story(story_in_project(&story_id)?)),
        StoryAction::TransitionHistory { story_id } => {
            story_in_project(&story_id)?;
            Ok(StoryReply::Transitions(
                store.transition_history(&story_id)?,
            ))
        }
        StoryAction::AddDependency {
            story_id,
            dependency_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            story_in_project(&dependency_id)?;
            Ok(StoryReply::Story(store.add_dependency(
                &story_id,
                &dependency_id,
                expected_revision,
            )?))
        }
        StoryAction::RemoveDependency {
            story_id,
            dependency_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            story_in_project(&dependency_id)?;
            Ok(StoryReply::Story(store.remove_dependency(
                &story_id,
                &dependency_id,
                expected_revision,
                actor_session,
            )?))
        }
        StoryAction::Claim {
            story_id,
            expected_revision,
        } => {
            story_in_project(&story_id)?;
            let session = actor_session.ok_or("claim requires a live session")?;
            Ok(StoryReply::Story(store.claim(
                &story_id,
                session,
                expected_revision,
            )?))
        }
        StoryAction::Transition {
            story_id,
            expected_revision,
            command,
        } => {
            story_in_project(&story_id)?;
            let changed = if unauthenticated_http && actor_session.is_none() {
                store.transition_from_local_api(&story_id, expected_revision, command)?
            } else {
                store.transition_for_actor(&story_id, expected_revision, command, actor_session)?
            };
            Ok(StoryReply::Story(changed))
        }
    }
}

/// A claim is bound to a live PTY in the same registered project.
pub fn story_action_for_session(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
) -> Result<StoryReply, String> {
    story_action_for_session_with_source(state, project, action, session_id, false)
}

/// Sessionless HTTP transitions record LocalApi provenance. A managed session
/// may approve only a story claimed by a different session.
pub fn story_action_for_http(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
) -> Result<StoryReply, String> {
    story_action_for_session_with_source(state, project, action, session_id, true)
}

fn story_action_for_session_with_source(
    state: &crate::AppState,
    project: &str,
    action: StoryAction,
    session_id: Option<&str>,
    unauthenticated_http: bool,
) -> Result<StoryReply, String> {
    if let Some(session) = session_id {
        let session_project = crate::progress::project_for_session(state, session)
            .ok_or("session has no registered project")?;
        let session_owner = crate::progress::resolve_owning_project(Some(&session_project))?;
        let requested_owner = crate::progress::resolve_owning_project(Some(project))?;
        if session_owner != requested_owner {
            return Err("session does not belong to project".into());
        }
    }
    if matches!(action, StoryAction::Claim { .. }) && session_id.is_none() {
        return Err("claim requires a live session".into());
    }
    story_action_with_source(project, action, session_id, unauthenticated_http)
}

#[cfg(test)]
mod tests {
    use super::super::StoryOrigin;
    use super::*;

    #[test]
    fn plan_sources_follow_repo_files_and_document_titles() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(repo.path().join("plans/archive")).expect("plan dirs");
        std::fs::create_dir_all(repo.path().join(".claude/plans")).expect("claude plans");
        std::fs::write(
            repo.path().join("plans/first.md"),
            "---\ntitle: Front title\n---\n# Other title\n",
        )
        .expect("frontmatter plan");
        std::fs::write(
            repo.path().join(".claude/plans/second.md"),
            "# Heading title\n",
        )
        .expect("heading plan");
        std::fs::write(repo.path().join("plans/archive/old.md"), "# Archived\n")
            .expect("archived plan");
        let project = repo.path().to_str().expect("project path");
        let list = || {
            let action = serde_json::from_value(serde_json::json!({"action":"list_plan_sources"}))
                .expect("list plan sources action");
            serde_json::to_value(story_action(project, action, None).expect("plan sources"))
                .expect("serialize sources")
        };
        let first = list();
        assert_eq!(first["type"], "plan_sources");
        assert_eq!(first["value"].as_array().expect("sources").len(), 2);
        assert!(
            first["value"]
                .as_array()
                .expect("sources")
                .iter()
                .any(|item| item["source"] == "plans/first.md" && item["title"] == "Front title")
        );
        assert!(
            first["value"]
                .as_array()
                .expect("sources")
                .iter()
                .any(|item| item["source"] == ".claude/plans/second.md"
                    && item["title"] == "Heading title")
        );

        std::fs::write(repo.path().join("plans/new.md"), "# Created by an agent\n")
            .expect("new plan");
        assert!(
            list()["value"]
                .as_array()
                .expect("refreshed sources")
                .iter()
                .any(|item| item["source"] == "plans/new.md"
                    && item["title"] == "Created by an agent")
        );
    }

    #[test]
    fn adding_a_plan_source_derives_its_title_and_reuses_its_record() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tempfile::tempdir().expect("project");
        std::fs::create_dir(repo.path().join("plans")).expect("plans dir");
        std::fs::write(repo.path().join("plans/feature.md"), "# Document title\n").expect("plan");
        let project = repo.path().to_str().expect("project path");
        let action = || {
            serde_json::from_value(serde_json::json!({
                "action":"add_plan_source", "source":"plans/feature.md"
            }))
            .expect("add plan source action")
        };
        let StoryReply::Plan(first) = story_action(project, action(), None).expect("first add")
        else {
            panic!("expected plan");
        };
        assert_eq!(first.title, "Document title");
        let StoryReply::Plan(second) = story_action(project, action(), None).expect("second add")
        else {
            panic!("expected plan");
        };
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn story_actions_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<StoryAction>(serde_json::json!({
                "action": "get_story", "story_id": "s", "project": "/other"
            }))
            .is_err()
        );
    }

    #[test]
    fn project_scopes_all_plan_and_story_reads() {
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let first = tempfile::tempdir().expect("first project");
        let second = tempfile::tempdir().expect("second project");
        let first_path = first.path().to_str().expect("first path");
        let second_path = second.path().to_str().expect("second path");

        let StoryReply::Plan(plan) = story_action(
            first_path,
            StoryAction::CreatePlan {
                title: "Plan".into(),
                source: "plans/feature.md".into(),
            },
            None,
        )
        .expect("create plan") else {
            panic!("expected plan");
        };
        assert!(matches!(
            story_action(
                &format!("{first_path}/."),
                StoryAction::GetPlan {
                    plan_id: plan.id.clone()
                },
                None,
            ),
            Ok(StoryReply::Plan(_))
        ));
        assert!(matches!(
            story_action(
                second_path,
                StoryAction::GetPlan {
                    plan_id: plan.id.clone()
                },
                None
            ),
            Err(_)
        ));
        assert!(
            matches!(story_action(second_path, StoryAction::ListPlans, None).expect("list"), StoryReply::Plans(plans) if plans.is_empty())
        );

        let StoryReply::Story(story) = story_action(
            first_path,
            StoryAction::CreateStory {
                input: NewStory {
                    plan_id: plan.id.clone(),
                    title: "Work".into(),
                    criteria: vec!["Done".into()],
                    priority: 1,
                    origin: StoryOrigin::Native,
                    file_scope: vec![],
                },
            },
            None,
        )
        .expect("create story") else {
            panic!("expected story");
        };
        assert!(matches!(
            story_action(
                second_path,
                StoryAction::GetStory { story_id: story.id },
                None
            ),
            Err(_)
        ));
    }
}
