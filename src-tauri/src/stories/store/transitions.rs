use super::*;
use std::collections::HashSet;

fn wrong_status(story: &Story, required: &str) -> String {
    format!(
        "story must be {required} for this command (status: {})",
        story.status.as_str()
    )
}

/// The wire name of a command, as an agent writes it in the request.
fn command_name(command: &StoryCommand) -> String {
    match serde_json::to_value(command) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{command:?}"),
    }
}

impl StoryStore {
    /// Rebuild dependent readiness from durable integration receipts. Safe to
    /// repeat after a crash between the run event and the story projection.
    pub fn reconcile_integrated_dependencies(&self, plan_id: &str) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        reconcile_ready(&tx, plan_id, &self.db_path)?;
        tx.commit()
            .map_err(|error| format!("commit dependency release: {error}"))
    }

    pub fn transition_history(&self, story_id: &str) -> Result<Vec<StoryTransition>, String> {
        let conn = self.connect()?;
        read_story(&conn, story_id)?;
        let mut stmt = conn.prepare(
            "SELECT revision,command_json,actor_json FROM story_transitions WHERE story_id=?1 ORDER BY revision"
        ).map_err(|e| format!("prepare story transition history: {e}"))?;
        let rows = stmt
            .query_map([story_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("read story transition history: {e}"))?;
        rows.map(|row| {
            let (revision, command_json, actor_json) =
                row.map_err(|e| format!("read story transition: {e}"))?;
            Ok(StoryTransition {
                story_id: story_id.into(),
                revision,
                command: serde_json::from_str(&command_json)
                    .map_err(|e| format!("decode story command: {e}"))?,
                actor: serde_json::from_str(&actor_json)
                    .map_err(|e| format!("decode story actor: {e}"))?,
            })
        })
        .collect()
    }

    pub fn add_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        let dependency = read_story(&tx, dependency_id)?;
        check_revision(&story, expected_revision)?;
        if story.plan_id != dependency.plan_id {
            return Err("dependencies must belong to one plan".into());
        }
        if !matches!(story.status, StoryStatus::Backlog | StoryStatus::Ready) {
            return Err(format!(
                "dependencies can be edited only on backlog or ready stories (status: {})",
                story.status.as_str()
            ));
        }
        if story.dependencies.iter().any(|id| id == dependency_id) {
            return Err(format!(
                "dependency already exists: {dependency_id} is already a dependency of {story_id}"
            ));
        }
        let mut seen = HashSet::new();
        let mut stack = vec![dependency_id.to_string()];
        while let Some(id) = stack.pop() {
            if id == story_id {
                return Err(format!(
                    "dependency cycle: {dependency_id} already depends on {story_id}"
                ));
            }
            if seen.insert(id.clone()) {
                stack.extend(read_story(&tx, &id)?.dependencies);
            }
        }
        story.dependencies.push(dependency_id.into());
        if !dependencies_integrated(&tx, &story, &self.db_path)? {
            // A ready story with an unfinished or unintegrated dependency is no longer claimable.
            story.status = StoryStatus::Backlog;
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit().map_err(|e| format!("commit dependency: {e}"))?;
        Ok(story)
    }

    pub fn remove_dependency(
        &self,
        story_id: &str,
        dependency_id: &str,
        expected_revision: i64,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        if actor_session.is_some() {
            return Err("remove_dependency is user-only and requires a user action: an agent session cannot remove a dependency; ask the user to remove it from the Plans and Stories dialog".into());
        }
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        if story.status != StoryStatus::Backlog {
            return Err(format!(
                "dependencies can be removed only from backlog stories (status: {})",
                story.status.as_str()
            ));
        }
        if !story.dependencies.iter().any(|id| id == dependency_id) {
            return Err(format!(
                "dependency does not exist on story: {dependency_id} is not listed in its dependencies"
            ));
        }
        let dependency = read_story(&tx, dependency_id)?;
        if dependency.plan_id != story.plan_id || dependency.status != StoryStatus::WontFix {
            return Err(format!(
                "only cancelled (wont_fix) dependencies in the same plan can be removed; {dependency_id} is {}",
                if dependency.plan_id != story.plan_id {
                    "in another plan"
                } else {
                    dependency.status.as_str()
                }
            ));
        }
        story.dependencies.retain(|id| id != dependency_id);
        if dependencies_integrated(&tx, &story, &self.db_path)? {
            story.status = StoryStatus::Ready;
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit()
            .map_err(|e| format!("commit dependency removal: {e}"))?;
        Ok(story)
    }

    pub fn transition(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
    ) -> Result<Story, String> {
        self.transition_for_actor(story_id, expected_revision, command, None)
    }

    pub fn transition_for_actor(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let actor = actor_session.map_or(StoryTransitionActor::Human, |session_id| {
            StoryTransitionActor::ManagedSession {
                session_id: session_id.into(),
            }
        });
        self.transition_as(story_id, expected_revision, command, actor)
    }

    pub(crate) fn transition_from_local_api(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
    ) -> Result<Story, String> {
        self.transition_as(
            story_id,
            expected_revision,
            command,
            StoryTransitionActor::LocalApi,
        )
    }

    fn transition_as(
        &self,
        story_id: &str,
        expected_revision: i64,
        command: StoryCommand,
        actor: StoryTransitionActor,
    ) -> Result<Story, String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        if let StoryTransitionActor::ManagedSession { session_id: actor } = &actor {
            match command {
                StoryCommand::CheckCriterion(_)
                | StoryCommand::UncheckCriterion(_)
                | StoryCommand::SubmitReview => {
                    if story.claim_session.as_deref() != Some(actor) {
                        return Err(match story.claim_session.as_deref() {
                            Some(_) => "story is claimed by another session".to_string(),
                            None => format!(
                                "story is not claimed (status: {}); claim it first",
                                story.status.as_str()
                            ),
                        });
                    }
                }
                StoryCommand::Approve => {
                    if story.claim_session.as_deref() == Some(actor) {
                        return Err("a story cannot be approved by its implementer".into());
                    }
                }
                _ => {
                    return Err(format!(
                        "{} is user-only and requires a user action: an agent session may only check_criterion, uncheck_criterion and submit_review on its own claimed story, or approve a story claimed by a different session; ask the user to perform it from the Plans and Stories dialog",
                        command_name(&command)
                    ));
                }
            }
        }
        match command {
            StoryCommand::StartManual => {
                if story.status != StoryStatus::Ready {
                    return Err(wrong_status(&story, "ready"));
                }
                if !dependencies_integrated(&tx, &story, &self.db_path)? {
                    return Err("story dependency lacks a current integration receipt".into());
                }
                story.status = StoryStatus::InProgress;
            }
            StoryCommand::CheckCriterion(index) | StoryCommand::UncheckCriterion(index) => {
                if story.status != StoryStatus::InProgress {
                    return Err(wrong_status(&story, "in_progress"));
                }
                let checked = story.checked.get_mut(index).ok_or_else(|| {
                    format!(
                        "criterion index {index} out of range: story has {} criteria",
                        story.criteria.len()
                    )
                })?;
                *checked = matches!(command, StoryCommand::CheckCriterion(_));
            }
            StoryCommand::SubmitReview => {
                if story.status != StoryStatus::InProgress {
                    return Err(wrong_status(&story, "in_progress"));
                }
                if !story.checked.iter().all(|checked| *checked) {
                    let open: Vec<String> = story
                        .checked
                        .iter()
                        .enumerate()
                        .filter(|(_, checked)| !**checked)
                        .map(|(index, _)| index.to_string())
                        .collect();
                    return Err(format!(
                        "criteria are incomplete: unchecked indexes {}",
                        open.join(", ")
                    ));
                }
                story.status = StoryStatus::Review;
            }
            StoryCommand::Approve => {
                if story.status != StoryStatus::Review {
                    return Err(wrong_status(&story, "review"));
                }
                story.status = StoryStatus::Done;
                story.claim_session = None;
            }
            StoryCommand::RejectReview => {
                if story.status != StoryStatus::Review {
                    return Err(wrong_status(&story, "review"));
                }
                story.status = if story.claim_session.is_some() {
                    StoryStatus::InProgress
                } else {
                    StoryStatus::Ready
                };
            }
            StoryCommand::Block => {
                if !matches!(
                    story.status,
                    StoryStatus::Ready | StoryStatus::InProgress | StoryStatus::Review
                ) {
                    return Err(format!(
                        "story can be blocked only from ready, in_progress or review (status: {})",
                        story.status.as_str()
                    ));
                }
                story.status = StoryStatus::Blocked;
                story.claim_session = None;
            }
            StoryCommand::Unblock => {
                if story.status != StoryStatus::Blocked {
                    return Err(wrong_status(&story, "blocked"));
                }
                story.status = if dependencies_integrated(&tx, &story, &self.db_path)? {
                    StoryStatus::Ready
                } else {
                    StoryStatus::Backlog
                };
            }
            StoryCommand::WontFix => {
                if story.status == StoryStatus::Done {
                    return Err("completed story cannot be discarded".into());
                }
                if story.status == StoryStatus::WontFix {
                    return Err("story is already cancelled".into());
                }
                story.status = StoryStatus::WontFix;
                story.claim_session = None;
            }
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.execute(
            "INSERT INTO story_transitions(story_id,revision,command_json,actor_json) VALUES (?1,?2,?3,?4)",
            rusqlite::params![
                story.id, story.revision,
                serde_json::to_string(&command).map_err(|e| format!("encode story command: {e}"))?,
                serde_json::to_string(&actor)
                    .map_err(|e| format!("encode story actor: {e}"))?,
            ],
        ).map_err(|e| format!("record story transition: {e}"))?;
        if story.status == StoryStatus::Done {
            reconcile_ready(&tx, &story.plan_id, &self.db_path)?;
        }
        tx.commit()
            .map_err(|e| format!("commit story transition: {e}"))?;
        Ok(story)
    }
}
