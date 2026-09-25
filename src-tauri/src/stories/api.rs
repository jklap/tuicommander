use super::{NewPlan, NewStory, Plan, PlanState, Story, StoryCommand, StoryStore};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoryAction {
    CreatePlan {
        title: String,
        source: String,
    },
    ListPlans,
    GetPlan {
        plan_id: String,
    },
    PlanState {
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
    PlanState(PlanState),
    Story(Story),
    Stories(Vec<Story>),
}

/// All transports use this boundary, so an identifier alone never grants cross-project access.
pub fn story_action(
    project: &str,
    action: StoryAction,
    actor_session: Option<&str>,
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
        StoryAction::GetPlan { plan_id } => Ok(StoryReply::Plan(plan_in_project(&plan_id)?)),
        StoryAction::PlanState { plan_id } => {
            plan_in_project(&plan_id)?;
            Ok(StoryReply::PlanState(store.plan_state(&plan_id)?))
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
            Ok(StoryReply::Story(store.transition_for_actor(
                &story_id,
                expected_revision,
                command,
                actor_session,
            )?))
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
    story_action(project, action, session_id)
}

#[cfg(test)]
mod tests {
    use super::super::StoryOrigin;
    use super::*;

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
