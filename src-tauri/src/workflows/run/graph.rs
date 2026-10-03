//! Versioned graph projections. No scheduler or external effects run here.
use crate::workflows::{NodeKind, PublishedWorkflow, validate_executable_graph};
use serde::{Deserialize, Serialize};

/// First graph contract; absent graph executions identify legacy record-only runs.
pub const GRAPH_CONTRACT_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    Ready,
    Running,
    Completed,
    Paused,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Activation {
    pub id: String,
    pub node_id: String,
    pub generation: u64,
    pub state: ActivationState,
    pub input_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgeToken {
    pub id: String,
    pub from_activation: String,
    pub edge_index: Option<usize>,
    pub target: String,
    pub consumed_by: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeOutcome {
    Yes,
    No,
    Uncertain,
    Pass,
    Fail,
    Completed,
    Blocked,
}

impl EdgeOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Uncertain => "uncertain",
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }
}

/// This records provenance; it does not authorize story approval (slice D).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionActor {
    Session { session_id: String },
    Operator { authorization_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionEvidence {
    pub actor: DecisionActor,
    pub reason: String,
    pub references: Vec<String>,
}

impl DecisionEvidence {
    fn validate(&self) -> Result<(), String> {
        let actor = match &self.actor {
            DecisionActor::Session { session_id } => session_id,
            DecisionActor::Operator { authorization_id } => authorization_id,
        };
        if actor.trim().is_empty()
            || actor.len() > 256
            || self.reason.trim().is_empty()
            || self.reason.len() > 4096
            || self.references.is_empty()
            || self.references.len() > 32
            || self
                .references
                .iter()
                .any(|reference| reference.trim().is_empty() || reference.len() > 4096)
        {
            return Err(
                "graph decision needs bounded actor, reason and evidence references".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphDecision {
    pub activation_id: String,
    pub edge_index: usize,
    pub evidence: DecisionEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoopCounter {
    pub node_id: String,
    pub repeats: u16,
}

/// A typed resolution does not implicitly authorize a check, approval or merge.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PauseResolution {
    Input { answer: String },
    Evidence { references: Vec<String> },
    Retry { reason: String },
}

impl PauseResolution {
    fn validate(&self) -> Result<(), String> {
        let values = match self {
            Self::Input { answer } => std::slice::from_ref(answer),
            Self::Retry { reason } => std::slice::from_ref(reason),
            Self::Evidence { references } => references.as_slice(),
        };
        if values.is_empty()
            || values.len() > 32
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 4096)
        {
            return Err("pause resolution is empty or too large".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphPause {
    pub activation_id: String,
    pub resume_to: String,
    pub evidence: DecisionEvidence,
    pub resolution: Option<PauseResolution>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphExecution {
    pub contract_version: u16,
    pub id: String,
    pub target_id: String,
    pub definition: PublishedWorkflow,
    pub activations: Vec<Activation>,
    pub tokens: Vec<EdgeToken>,
    pub decisions: Vec<GraphDecision>,
    pub loops: Vec<LoopCounter>,
    pub pauses: Vec<GraphPause>,
    pub completed: bool,
}

impl GraphExecution {
    pub(super) fn start(
        id: String,
        target_id: String,
        definition: PublishedWorkflow,
    ) -> Result<Self, String> {
        if id.trim().is_empty()
            || id.len() > 80
            || target_id.trim().is_empty()
            || definition.revision < 1
        {
            return Err("invalid graph execution identity or revision".into());
        }
        validate_executable_graph(
            &definition.graph,
            definition.kind,
            !definition.required_checks.is_empty(),
        )?;
        let start = definition
            .graph
            .nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Start))
            .ok_or("graph Start is missing")?;
        let root = Activation {
            id: "a0".into(),
            node_id: start.id.clone(),
            generation: 1,
            state: ActivationState::Ready,
            input_token: None,
        };
        Ok(Self {
            contract_version: GRAPH_CONTRACT_VERSION,
            id,
            target_id,
            definition,
            activations: vec![root],
            tokens: vec![],
            decisions: vec![],
            loops: vec![],
            pauses: vec![],
            completed: false,
        })
    }

    fn enqueue(
        &mut self,
        from: &str,
        edge_index: Option<usize>,
        target: String,
    ) -> Result<(), String> {
        if self.activations.len() >= 4096 {
            return Err("graph activation budget exhausted".into());
        }
        let id = format!("a{}", self.activations.len());
        let token_id = format!("t{}", self.tokens.len());
        let generation = u64::try_from(
            self.activations
                .iter()
                .filter(|a| a.node_id == target)
                .count(),
        )
        .map_err(|_| "graph generation overflow")?
        .checked_add(1)
        .ok_or("graph generation overflow")?;
        self.tokens.push(EdgeToken {
            id: token_id.clone(),
            from_activation: from.into(),
            edge_index,
            target: target.clone(),
            consumed_by: None,
        });
        self.activations.push(Activation {
            id,
            node_id: target,
            generation,
            state: ActivationState::Ready,
            input_token: Some(token_id),
        });
        Ok(())
    }

    /// Validate and project a committed transition without performing effects.
    pub(super) fn apply(
        &mut self,
        transition: &GraphTransition,
        remaining_loops: u16,
    ) -> Result<u16, String> {
        if self.contract_version != GRAPH_CONTRACT_VERSION {
            return Err("unsupported graph contract version".into());
        }
        if self.completed {
            return Err("completed graph cannot advance".into());
        }
        let activation_id = match transition {
            GraphTransition::Activate { activation_id, .. }
            | GraphTransition::Complete { activation_id, .. }
            | GraphTransition::ResolvePause { activation_id, .. } => activation_id,
            GraphTransition::Start { .. } => return Err("graph execution already exists".into()),
        };
        let index = self
            .activations
            .iter()
            .position(|a| a.id == *activation_id)
            .ok_or("graph activation not reached")?;
        let activation = self.activations[index].clone();
        let kind = self
            .definition
            .graph
            .nodes
            .iter()
            .find(|node| node.id == activation.node_id)
            .ok_or("activation node is missing")?
            .kind
            .clone();
        let paused = self.pauses.iter().any(|pause| pause.resolution.is_none());
        match transition {
            GraphTransition::Activate { .. } => {
                if paused || activation.state != ActivationState::Ready {
                    return Err("graph activation is not ready".into());
                }
                // Fork/Join all execution is deliberately unavailable until slice G.
                if matches!(
                    kind,
                    NodeKind::Fork { .. }
                        | NodeKind::Join {
                            mode: crate::workflows::JoinMode::All,
                            ..
                        }
                ) {
                    return Err("parallel graph execution requires slice G".into());
                }
                if let Some(token_id) = &activation.input_token {
                    let token = self
                        .tokens
                        .iter_mut()
                        .find(|token| token.id == *token_id)
                        .ok_or("activation predecessor token is missing")?;
                    if token.target != activation.node_id || token.consumed_by.is_some() {
                        return Err("activation predecessor token is not available".into());
                    }
                    token.consumed_by = Some(activation.id.clone());
                } else if !matches!(kind, NodeKind::Start) || index != 0 {
                    return Err("non-root activation needs a predecessor token".into());
                }
                self.activations[index].state = ActivationState::Running;
            }
            GraphTransition::Complete {
                outcome, evidence, ..
            } => {
                if paused || activation.state != ActivationState::Running {
                    return Err("graph activation is not running".into());
                }
                if let NodeKind::Pause { resume_to } = kind {
                    if outcome.is_some() {
                        return Err("Pause has no outcome edge".into());
                    }
                    let evidence = evidence.as_ref().ok_or("Pause needs a recorded reason")?;
                    evidence.validate()?;
                    self.pauses.push(GraphPause {
                        activation_id: activation.id.clone(),
                        resume_to: resume_to.ok_or("Pause has no resume_to")?,
                        evidence: evidence.clone(),
                        resolution: None,
                    });
                    self.activations[index].state = ActivationState::Paused;
                    return Ok(0);
                }
                if matches!(kind, NodeKind::End) {
                    if outcome.is_some() || evidence.is_some() {
                        return Err("End has no outcome".into());
                    }
                    self.activations[index].state = ActivationState::Completed;
                    self.completed = true;
                    return Ok(0);
                }
                let mut repeats = 0;
                let label = match kind {
                    NodeKind::Judge | NodeKind::Gate | NodeKind::StoryDispatch { .. } => {
                        let evidence = evidence.as_ref().ok_or("decision needs evidence")?;
                        evidence.validate()?;
                        Some(outcome.ok_or("decision needs one outcome")?.label())
                    }
                    NodeKind::Loop { max_iterations } => {
                        if outcome.is_some() || evidence.is_some() {
                            return Err("Loop outcome is computed from its pinned cap".into());
                        }
                        let used = self
                            .loops
                            .iter()
                            .find(|counter| counter.node_id == activation.node_id)
                            .map_or(0, |counter| counter.repeats);
                        if used < max_iterations && remaining_loops > 0 {
                            repeats = 1;
                            Some("repeat")
                        } else {
                            Some("exhausted")
                        }
                    }
                    _ => {
                        if outcome.is_some() || evidence.is_some() {
                            return Err("node needs an unlabeled completion".into());
                        }
                        None
                    }
                };
                let (edge_index, edge) = self
                    .definition
                    .graph
                    .edges
                    .iter()
                    .enumerate()
                    .find(|(_, edge)| {
                        edge.from == activation.node_id && edge.outcome.as_deref() == label
                    })
                    .ok_or("outcome has no published edge")?;
                let target = edge.to.clone();
                if let Some(evidence) = evidence {
                    self.decisions.push(GraphDecision {
                        activation_id: activation.id.clone(),
                        edge_index,
                        evidence: evidence.clone(),
                    });
                }
                if repeats > 0 {
                    if let Some(counter) = self
                        .loops
                        .iter_mut()
                        .find(|counter| counter.node_id == activation.node_id)
                    {
                        counter.repeats = counter
                            .repeats
                            .checked_add(1)
                            .ok_or("loop counter overflow")?;
                    } else {
                        self.loops.push(LoopCounter {
                            node_id: activation.node_id.clone(),
                            repeats: 1,
                        });
                    }
                }
                self.enqueue(&activation.id, Some(edge_index), target)?;
                self.activations[index].state = ActivationState::Completed;
                return Ok(repeats);
            }
            GraphTransition::ResolvePause { resolution, .. } => {
                resolution.validate()?;
                if activation.state != ActivationState::Paused {
                    return Err("activation has no pending pause".into());
                }
                let pause_index = self
                    .pauses
                    .iter()
                    .position(|pause| {
                        pause.activation_id == activation.id && pause.resolution.is_none()
                    })
                    .ok_or("activation has no pending pause")?;
                let target = self.pauses[pause_index].resume_to.clone();
                self.enqueue(&activation.id, None, target)?;
                self.pauses[pause_index].resolution = Some(resolution.clone());
                self.activations[index].state = ActivationState::Completed;
            }
            GraphTransition::Start { .. } => unreachable!(),
        }
        Ok(0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphTransition {
    Start {
        execution_id: String,
        target_id: String,
    },
    Activate {
        execution_id: String,
        activation_id: String,
    },
    Complete {
        execution_id: String,
        activation_id: String,
        outcome: Option<EdgeOutcome>,
        evidence: Option<DecisionEvidence>,
    },
    ResolvePause {
        execution_id: String,
        activation_id: String,
        resolution: PauseResolution,
    },
}

impl GraphTransition {
    pub(super) fn execution_id(&self) -> &str {
        match self {
            Self::Start { execution_id, .. }
            | Self::Activate { execution_id, .. }
            | Self::Complete { execution_id, .. }
            | Self::ResolvePause { execution_id, .. } => execution_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphEvent {
    Started { execution: Box<GraphExecution> },
    Transition { transition: GraphTransition },
}
