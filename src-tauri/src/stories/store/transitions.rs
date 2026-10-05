use super::*;
use std::collections::HashSet;

fn wrong_status(story: &Story, required: &str) -> String {
    format!(
        "story must be {required} for this command (status: {})",
        story.status.as_str()
    )
}

impl StoryStore {
    /// Rebuild dependent readiness from durable integration receipts. Safe to
    /// repeat after a crash between the run event and the story projection.
    pub fn reconcile_integrated_dependencies(&self, plan_id: &str) -> Result<(), String> {
        let preflight_plan = plan_id.to_owned();
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        reconcile_ready(&tx, plan_id, &preflight)?;
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
        let preflight_plan = self.get_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
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
        if !dependencies_integrated(&tx, &story, &preflight)? {
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
        _actor_session: Option<&str>,
    ) -> Result<Story, String> {
        let preflight_plan = self.get_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
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
        if dependencies_integrated(&tx, &story, &preflight)? {
            story.status = StoryStatus::Ready;
        }
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit()
            .map_err(|e| format!("commit dependency removal: {e}"))?;
        Ok(story)
    }

    #[cfg(test)]
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
        let preflight_plan = self.get_story(story_id)?.plan_id;
        let preflight = DependencyPreflight::prepare(self, &preflight_plan)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        preflight.validate(&tx)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        match command {
            StoryCommand::StartManual => {
                if story.status != StoryStatus::Ready {
                    return Err(wrong_status(&story, "ready"));
                }
                if !dependencies_integrated(&tx, &story, &preflight)? {
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
                story.status = if dependencies_integrated(&tx, &story, &preflight)? {
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
            reconcile_ready(&tx, &story.plan_id, &preflight)?;
        }
        tx.commit()
            .map_err(|e| format!("commit story transition: {e}"))?;
        Ok(story)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_story(dir: &std::path::Path, criteria: usize) -> (StoryStore, Story) {
        let store = StoryStore::open_at(&dir.join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Transitions".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Story".into(),
                criteria: (0..criteria).map(|n| format!("Criterion {n}")).collect(),
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        (store, story)
    }

    /// Catches: wrong_status returning an empty or placeholder string (mutants
    /// `String::new()` and `"xyzzy"` at the format! call), which would leave an agent
    /// refused with no hint of the status it needs.
    #[test]
    fn wrong_status_names_required_and_current_status() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);

        let err = store
            .transition(&story.id, story.revision, StoryCommand::Approve)
            .expect_err("approve needs review");
        assert_eq!(err, "story must be review for this command (status: ready)");
        let err = store
            .transition(&story.id, story.revision, StoryCommand::CheckCriterion(0))
            .expect_err("check needs in_progress");
        assert_eq!(
            err,
            "story must be in_progress for this command (status: ready)"
        );
    }

    /// Catches: actor provenance being used to refuse another caller's valid criterion update.
    #[test]
    fn criterion_commands_use_status_and_revision_not_actor_authority() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);
        let err = store
            .transition_for_actor(
                &story.id,
                story.revision,
                StoryCommand::CheckCriterion(0),
                Some("impl"),
            )
            .expect_err("not in progress");
        assert!(err.contains("in_progress"), "{err}");
        let claimed = store
            .claim(&story.id, "impl", story.revision)
            .expect("claim");
        let checked = store
            .transition_for_actor(
                &story.id,
                claimed.revision,
                StoryCommand::CheckCriterion(0),
                Some("other"),
            )
            .expect("caller checks");
        assert_eq!(checked.checked, vec![true]);
        let unchecked = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::UncheckCriterion(0),
                Some("impl"),
            )
            .expect("holder unchecks");
        assert_eq!(unchecked.checked, vec![false]);
        let checked = store
            .transition_for_actor(
                &story.id,
                unchecked.revision,
                StoryCommand::CheckCriterion(0),
                Some("impl"),
            )
            .expect("holder checks again");
        let review = store
            .transition_for_actor(
                &story.id,
                checked.revision,
                StoryCommand::SubmitReview,
                Some("impl"),
            )
            .expect("holder submits");
        assert_eq!(review.status, StoryStatus::Review);
    }

    /// Catches: deleting the `!` in the unchecked-criteria filter, which would report the
    /// checked indexes as the ones still open.
    #[test]
    fn pending_criteria_lists_only_unchecked_indices() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 3);
        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let s = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check 0");
        let s = store
            .transition(&story.id, s.revision, StoryCommand::CheckCriterion(2))
            .expect("check 2");

        let err = store
            .transition(&story.id, s.revision, StoryCommand::SubmitReview)
            .expect_err("criterion 1 is open");
        assert_eq!(err, "criteria are incomplete: unchecked indexes 1");
    }

    /// Catches: `!=` flipped to `==` on the RejectReview (needs Review) and Unblock (needs
    /// Blocked) guards: either command would refuse its only valid status and run anywhere else.
    #[test]
    fn reject_review_outside_review_is_refused_and_unblock_outside_blocked_is_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let (store, story) = store_with_story(dir.path(), 1);

        let err = store
            .transition(&story.id, story.revision, StoryCommand::RejectReview)
            .expect_err("ready story has no review to reject");
        assert_eq!(err, "story must be review for this command (status: ready)");
        let err = store
            .transition(&story.id, story.revision, StoryCommand::Unblock)
            .expect_err("ready story is not blocked");
        assert_eq!(
            err,
            "story must be blocked for this command (status: ready)"
        );

        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .expect("review");
        let rejected = store
            .transition(&story.id, review.revision, StoryCommand::RejectReview)
            .expect("review is rejectable");
        assert_eq!(rejected.status, StoryStatus::Ready);

        let blocked = store
            .transition(&story.id, rejected.revision, StoryCommand::Block)
            .expect("block");
        let unblocked = store
            .transition(&story.id, blocked.revision, StoryCommand::Unblock)
            .expect("blocked story unblocks");
        assert_eq!(unblocked.status, StoryStatus::Ready);
    }
}
