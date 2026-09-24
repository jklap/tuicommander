use super::model::*;

/// Pure projection of a committed event. Replaying the event stream must rebuild
/// the exact snapshot stored by the writer transaction.
pub fn apply_event(previous: Option<RunSnapshot>, event: &RunEvent) -> Result<RunSnapshot, String> {
    if let RunEventKind::Started { initial } = &event.kind {
        if previous.is_some() || event.sequence != 1 || initial.sequence != 0 {
            return Err("invalid workflow start event".into());
        }
        let mut snapshot = (**initial).clone();
        snapshot.sequence = event.sequence;
        return Ok(snapshot);
    }
    let mut snapshot = previous.ok_or("workflow event has no start")?;
    if event.sequence != snapshot.sequence + 1 {
        return Err("workflow event sequence gap".into());
    }
    if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
        return Err("terminal workflow cannot advance".into());
    }
    match &event.kind {
        RunEventKind::Started { .. } => unreachable!(),
        RunEventKind::PlanningClosed { fingerprint } => {
            snapshot.planning_fingerprint = Some(fingerprint.clone());
            snapshot.verification_fingerprint = None;
        }
        RunEventKind::PlanningReopened => {
            snapshot.planning_fingerprint = None;
            snapshot.verification_fingerprint = None;
        }
        RunEventKind::AttemptStarted { attempt } => {
            if snapshot
                .attempts
                .iter()
                .any(|existing| existing.id == attempt.id)
            {
                return Err("duplicate node attempt".into());
            }
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == attempt.story_id);
            if let Some(story) = story {
                story.attempt_ids.push(attempt.id.clone());
            } else {
                snapshot.stories.push(StoryExecution {
                    story_id: attempt.story_id.clone(),
                    accepted: false,
                    accepted_revision: None,
                    attempt_ids: vec![attempt.id.clone()],
                });
            }
            snapshot.attempts.push(attempt.clone());
        }
        RunEventKind::AttemptReported {
            attempt_id,
            generation,
            outcome,
        } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.generation != *generation || attempt.state != AttemptState::Running {
                return Err("attempt report is no longer current".into());
            }
            attempt.state = AttemptState::Reported;
            attempt.outcome = Some(*outcome);
        }
        RunEventKind::LateReportIgnored { .. } => {}
        RunEventKind::AttemptInterrupted { attempt_id } => {
            let attempt = snapshot
                .attempts
                .iter_mut()
                .find(|attempt| attempt.id == *attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.state != AttemptState::Running {
                return Err("attempt is no longer running".into());
            }
            attempt.state = AttemptState::Interrupted;
            attempt.outcome = Some(AttemptOutcome::Interrupted);
            snapshot.status = RunStatus::Paused;
        }
        RunEventKind::EffectReserved { effect } => {
            if snapshot
                .effects
                .iter()
                .any(|existing| existing.id == effect.id || existing.key == effect.key)
            {
                return Err("effect key already reserved".into());
            }
            match effect.kind {
                EffectKind::SpawnAgent => {
                    snapshot.spawns = snapshot
                        .spawns
                        .checked_add(1)
                        .ok_or("spawn counter overflow")?
                }
                EffectKind::CreateStory => {
                    snapshot.story_creations = snapshot
                        .story_creations
                        .checked_add(1)
                        .ok_or("story creation counter overflow")?
                }
                _ => {}
            }
            if snapshot.spawns > snapshot.limits.max_spawns
                || snapshot.story_creations > snapshot.limits.max_story_creations
            {
                return Err("workflow effect budget exhausted".into());
            }
            snapshot.effects.push(effect.clone());
        }
        RunEventKind::EffectChanged { effect_id, state } => {
            let effect = snapshot
                .effects
                .iter_mut()
                .find(|effect| effect.id == *effect_id)
                .ok_or("effect intent not found")?;
            if !matches!(
                (effect.state, *state),
                (
                    EffectState::Intended,
                    EffectState::Succeeded | EffectState::Failed | EffectState::Uncertain
                ) | (
                    EffectState::Uncertain,
                    EffectState::Succeeded | EffectState::Failed
                )
            ) {
                return Err("effect state cannot change this way".into());
            }
            effect.state = *state;
            if *state == EffectState::Uncertain {
                snapshot.status = RunStatus::Paused;
            }
        }
        RunEventKind::LoopAdvanced => {
            snapshot.loops = snapshot
                .loops
                .checked_add(1)
                .ok_or("loop counter overflow")?;
            if snapshot.loops > snapshot.limits.max_loops {
                return Err("workflow loop budget exhausted".into());
            }
        }
        RunEventKind::StoryAccepted { story_id, revision } => {
            let story = snapshot
                .stories
                .iter_mut()
                .find(|story| story.story_id == *story_id);
            if let Some(story) = story {
                story.accepted = true;
                story.accepted_revision = Some(*revision);
            } else {
                snapshot.stories.push(StoryExecution {
                    story_id: story_id.clone(),
                    accepted: true,
                    accepted_revision: Some(*revision),
                    attempt_ids: vec![],
                });
            }
        }
        RunEventKind::VerificationPassed { fingerprint } => {
            snapshot.verification_fingerprint = Some(fingerprint.clone())
        }
        RunEventKind::Paused => snapshot.status = RunStatus::Paused,
        RunEventKind::Resumed => snapshot.status = RunStatus::Running,
        RunEventKind::Cancelled => snapshot.status = RunStatus::Cancelled,
        RunEventKind::Completed => snapshot.status = RunStatus::Completed,
    }
    snapshot.sequence = event.sequence;
    Ok(snapshot)
}
