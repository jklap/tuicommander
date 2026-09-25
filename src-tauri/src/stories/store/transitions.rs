use super::*;
use std::collections::HashSet;

impl StoryStore {
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
            return Err("dependencies can be edited only before work starts".into());
        }
        if story.dependencies.iter().any(|id| id == dependency_id) {
            return Err("dependency already exists".into());
        }
        let mut seen = HashSet::new();
        let mut stack = vec![dependency_id.to_string()];
        while let Some(id) = stack.pop() {
            if id == story_id {
                return Err("dependency cycle".into());
            }
            if seen.insert(id.clone()) {
                stack.extend(read_story(&tx, &id)?.dependencies);
            }
        }
        story.dependencies.push(dependency_id.into());
        if dependency.status != StoryStatus::Done {
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
            return Err("dependency removal requires a user action".into());
        }
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        if story.status != StoryStatus::Backlog {
            return Err("dependencies can be removed only from backlog stories".into());
        }
        if !story.dependencies.iter().any(|id| id == dependency_id) {
            return Err("dependency does not exist on story".into());
        }
        let dependency = read_story(&tx, dependency_id)?;
        if dependency.plan_id != story.plan_id || dependency.status != StoryStatus::WontFix {
            return Err("only cancelled dependencies in the same plan can be removed".into());
        }
        story.dependencies.retain(|id| id != dependency_id);
        if dependencies_done(&tx, &story)? {
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
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        if let Some(actor) = actor_session {
            match command {
                StoryCommand::CheckCriterion(_)
                | StoryCommand::UncheckCriterion(_)
                | StoryCommand::SubmitReview => {
                    if story.claim_session.as_deref() != Some(actor) {
                        return Err("story is not claimed by calling session".into());
                    }
                }
                _ => {
                    return Err(
                        "review and administrative transitions require a user action".into(),
                    );
                }
            }
        }
        match command {
            StoryCommand::StartManual => {
                if story.status != StoryStatus::Ready {
                    return Err("story must be ready for manual work".into());
                }
                story.status = StoryStatus::InProgress;
            }
            StoryCommand::CheckCriterion(index) | StoryCommand::UncheckCriterion(index) => {
                if story.status != StoryStatus::InProgress {
                    return Err("criteria can change only during work".into());
                }
                let checked = story
                    .checked
                    .get_mut(index)
                    .ok_or("criterion index out of range")?;
                *checked = matches!(command, StoryCommand::CheckCriterion(_));
            }
            StoryCommand::SubmitReview => {
                if story.status != StoryStatus::InProgress {
                    return Err("story must be in progress".into());
                }
                if !story.checked.iter().all(|checked| *checked) {
                    return Err("criteria are incomplete".into());
                }
                story.status = StoryStatus::Review;
            }
            StoryCommand::Approve => {
                if story.status != StoryStatus::Review {
                    return Err("story must be in review".into());
                }
                story.status = StoryStatus::Done;
                story.claim_session = None;
            }
            StoryCommand::RejectReview => {
                if story.status != StoryStatus::Review {
                    return Err("story must be in review".into());
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
                    return Err("story cannot be blocked from this state".into());
                }
                story.status = StoryStatus::Blocked;
                story.claim_session = None;
            }
            StoryCommand::Unblock => {
                if story.status != StoryStatus::Blocked {
                    return Err("story is not blocked".into());
                }
                story.status = if dependencies_done(&tx, &story)? {
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
        if story.status == StoryStatus::Done {
            promote_ready(&tx, &story.plan_id)?;
        }
        tx.commit()
            .map_err(|e| format!("commit story transition: {e}"))?;
        Ok(story)
    }
}
