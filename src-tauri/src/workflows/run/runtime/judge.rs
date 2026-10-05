//! Routing consumes typed evidence; graph completion never fabricates approval.
use super::super::graph::{Activation, EdgeOutcome, GraphExecution};
use super::super::{AttemptOutcome, ReviewDecision};
use super::*;
use crate::workflows::AgentRole;

pub(in crate::workflows::run) fn judge(
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Result<(EdgeOutcome, DecisionEvidence), String> {
    let uncertain = |reason: &str| {
        (
            EdgeOutcome::Uncertain,
            DecisionEvidence {
                actor: "daemon".into(),
                reason: reason.into(),
                references: vec![format!("activation:{}:{}", graph.id, activation.id)],
            },
        )
    };
    let story = crate::stories::StoryStore::open()?.get_story(&graph.target_id)?;
    // A loop's previous review cannot decide a new implementation epoch.
    let epoch = graph
        .activations
        .iter()
        .rposition(|a| {
            graph.definition.graph.nodes.iter().any(|n| {
                n.id == a.node_id
                    && matches!(
                        n.kind,
                        NodeKind::Agent {
                            role: AgentRole::Implementer,
                            ..
                        }
                    )
            })
        })
        .unwrap_or(0);
    let mut current = graph.activations.iter().skip(epoch).filter_map(|a| {
        let node = graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| n.id == a.node_id)?;
        let NodeKind::Agent { role, .. } = node.kind else {
            return None;
        };
        let attempt = super::effects::activation_attempt(run, graph, a)?;
        Some((role, attempt))
    });
    let implementer = current
        .find(|(role, _)| *role == AgentRole::Implementer)
        .map(|(_, a)| a);
    let reviewer = graph.activations.iter().skip(epoch).rev().find_map(|a| {
        let node = graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| n.id == a.node_id)?;
        if !matches!(
            node.kind,
            NodeKind::Agent {
                role: AgentRole::Reviewer,
                ..
            }
        ) {
            return None;
        }
        super::effects::activation_attempt(run, graph, a)
    });
    let Some(reviewer) = reviewer else {
        return Ok(uncertain("No current reviewer report"));
    };
    let Some(report) = reviewer.report.as_ref() else {
        return Ok(uncertain("Reviewer did not provide a typed report"));
    };
    if report.story_revision != story.revision {
        return Ok(uncertain(
            "Reviewer evidence targets an older story revision",
        ));
    }
    let Some(review) = report.review.as_ref() else {
        return Ok(uncertain("Reviewer assessment is missing"));
    };
    let Some(binding) = reviewer.agent.as_ref() else {
        return Ok(uncertain("Reviewer identity is missing"));
    };
    let Some(implementer) = implementer else {
        return Ok(uncertain("Current implementation attempt is missing"));
    };
    if implementer
        .agent
        .as_ref()
        .is_none_or(|a| a.session_id == binding.session_id)
    {
        return Ok(uncertain("Review must come from a separate bound agent"));
    }
    if report.outcome != AttemptOutcome::Completed {
        return Ok(uncertain("Reviewer report is not completed"));
    }
    let Some(execution) = run.stories.iter().find(|s| s.story_id == story.id) else {
        return Ok(uncertain("Story artifact is missing"));
    };
    let Some(path) = execution.worktree_path.as_deref() else {
        return Ok(uncertain("Isolated artifact is missing"));
    };
    let (commit, tree) = match super::super::check::clean_artifact(Path::new(path)) {
        Ok(artifact) => artifact,
        Err(_) => return Ok(uncertain("Review artifact is dirty or unavailable")),
    };
    if review.artifact_digest != artifact_digest(&commit, &tree) {
        return Ok(uncertain("Review artifact moved"));
    }
    let evidence = DecisionEvidence {
        actor: binding.session_id.clone(),
        reason: report.summary.clone(),
        references: vec![
            format!("attempt:{}:{}", reviewer.id, reviewer.generation),
            format!("artifact:{commit}:{tree}"),
        ],
    };
    if review.decision == ReviewDecision::ChangesRequested {
        return Ok((EdgeOutcome::No, evidence));
    }
    if implementer
        .report
        .as_ref()
        .is_none_or(|r| r.outcome != AttemptOutcome::Completed)
        || story.status != crate::stories::StoryStatus::Done
        || !execution.accepted
        || execution.accepted_revision != Some(story.revision)
        || graph.definition.required_checks.is_empty()
        || !graph.definition.required_checks.iter().all(|check| {
            execution.check_receipts.iter().any(|r| {
                r.check_id == check.id
                    && r.argv == check.argv
                    && r.exit_code == 0
                    && r.commit == commit
                    && r.tree == tree
            })
        })
    {
        return Ok(uncertain(
            "Current independent approval and deterministic check receipts are required",
        ));
    }
    Ok((EdgeOutcome::Yes, evidence))
}

pub(in crate::workflows::run) use crate::workflows::prompt::review_artifact_digest as artifact_digest;
