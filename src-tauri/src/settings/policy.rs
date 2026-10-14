//! Host-issued origin and one-use approval authority. No stored values enter this pool.
use super::{catalogue::Catalogue, model::Sensitivity, request::*, transaction};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    net::SocketAddr,
    time::Duration,
};
use tokio::time::Instant;

const TTL: Duration = Duration::from_secs(300);
const MAX_PENDING: usize = 128;

/// Constructed by the server transport, never deserialized from a model payload.
#[derive(Clone)]
pub struct Caller {
    connection: String,
    allowed: bool,
    enabled: bool,
}
impl Caller {
    pub fn native() -> Self {
        Self {
            connection: "native".into(),
            allowed: true,
            enabled: true,
        }
    }
    pub fn http(connection: String, authenticated_or_local: bool) -> Self {
        Self {
            connection,
            allowed: authenticated_or_local,
            enabled: true,
        }
    }
    pub fn mcp(addr: SocketAddr, connection: String, settings_enabled: bool) -> Self {
        Self {
            connection,
            allowed: addr.ip().is_loopback(),
            enabled: settings_enabled,
        }
    }
    pub fn check(&self, mutation: bool) -> Result<(), Failure> {
        if !self.enabled {
            return Err(Error::new("settings", "unavailable"));
        }
        if mutation && !self.allowed {
            return Err(Error::new("settings", "forbidden_origin"));
        }
        Ok(())
    }
}

/// Only backend-built, redacted text may be offered to the confirmation dialog.
#[derive(Clone)]
pub struct Preview {
    pub title: String,
    pub message: String,
}
struct Proposal {
    connection: String,
    fingerprint: String,
    expires: Instant,
    preview: Preview,
    showing: bool,
    approved: bool,
}
#[derive(Default)]
struct State {
    proposals: BTreeMap<String, Proposal>,
    audit: VecDeque<Audit>,
}
/// Audit contains identifiers and outcomes only, never setting values or dynamic keys.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub connection: String,
    pub changed_ids: Vec<String>,
    pub outcome: String,
}
#[derive(Default)]
pub struct Policy {
    state: Mutex<State>,
}

impl Policy {
    /// Validate, describe and bind an exposure proposal without retaining secret inputs.
    pub(super) fn authorize(
        &self,
        cat: &Catalogue,
        caller: &Caller,
        request: &Request,
        latest: &Value,
        approval: Option<&str>,
    ) -> Result<(), Failure> {
        caller.check(true)?;
        let (target, store, revision, edits) = patch(request)?;
        let fingerprint = transaction::revision(&json!([target, store, revision, edits]))?;
        if let Some(id) = approval {
            let mut state = self.state.lock();
            let pending = state
                .proposals
                .remove(id)
                .ok_or_else(|| Error::new("settings", "confirmation_denied"))?;
            if pending.connection != caller.connection
                || pending.fingerprint != fingerprint
                || pending.expires <= Instant::now()
                || !pending.approved
            {
                return Err(Error::new("settings", "confirmation_denied"));
            }
            // Consume before revalidation: conflict/failure must never leave a reusable grant.
            drop(state);
            transaction::prepare(cat, store, latest, revision, &edits)?;
            return Ok(());
        }
        let prepared = transaction::prepare(cat, store, latest, revision, &edits)?;
        let fields: Vec<_> = prepared
            .reply
            .changed_ids
            .iter()
            .filter_map(|id| cat.fields.get(id))
            .collect();
        if fields
            .iter()
            .all(|f| f.annotation.confirmation_class == "ordinary")
        {
            return Ok(());
        }
        let message = fields
            .iter()
            .map(|f| {
                let render = |value: &Value| {
                    if f.annotation.sensitivity != Sensitivity::Public {
                        return "[redacted]".to_owned();
                    }
                    let text = value.pointer(&f.path).unwrap_or(&Value::Null).to_string();
                    if text.len() > 256 {
                        "[value omitted]".into()
                    } else {
                        text
                    }
                };
                format!("{}: {} → {}", f.id, render(latest), render(&prepared.value))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = self.state.lock();
        state.proposals.retain(|_, p| p.expires > Instant::now());
        if state.proposals.len() >= MAX_PENDING {
            return Err(Error::new("settings", "unavailable"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.proposals.insert(
            id.clone(),
            Proposal {
                connection: caller.connection.clone(),
                fingerprint,
                expires: Instant::now() + TTL,
                preview: Preview {
                    title: "Approve settings change".into(),
                    message,
                },
                showing: false,
                approved: false,
            },
        );
        state.audit.push_back(Audit {
            connection: caller.connection.clone(),
            changed_ids: prepared.reply.changed_ids,
            outcome: "confirmation_required".into(),
        });
        while state.audit.len() > 256 {
            state.audit.pop_front();
        }
        let mut error = Error::new(store, "confirmation_required");
        error.proposal_id = Some(id);
        error.next_action = "ui confirm proposalId".into();
        Err(error)
    }

    /// Lease a proposal to the real dialog once. This never grants approval itself.
    pub(crate) fn begin_confirmation(&self, caller: &Caller, id: &str) -> Result<Preview, Failure> {
        caller.check(true)?;
        let mut state = self.state.lock();
        let pending = state
            .proposals
            .get_mut(id)
            .ok_or_else(|| Error::new("settings", "confirmation_denied"))?;
        if pending.connection != caller.connection
            || pending.expires <= Instant::now()
            || pending.showing
            || pending.approved
        {
            return Err(Error::new("settings", "confirmation_denied"));
        }
        pending.showing = true;
        Ok(pending.preview.clone())
    }
    /// Called only after the existing human-response channel resolves.
    pub(crate) fn finish_confirmation(
        &self,
        caller: &Caller,
        id: &str,
        approved: bool,
    ) -> Result<(), Failure> {
        let mut state = self.state.lock();
        let Some(pending) = state.proposals.get_mut(id) else {
            return Err(Error::new("settings", "confirmation_denied"));
        };
        if !approved
            || pending.connection != caller.connection
            || pending.expires <= Instant::now()
            || !pending.showing
        {
            state.proposals.remove(id);
            return Err(Error::new("settings", "confirmation_denied"));
        }
        pending.approved = true;
        Ok(())
    }
    /// Teardown revokes pending and granted proposals for that protocol connection.
    pub fn disconnect(&self, connection: &str) {
        self.state
            .lock()
            .proposals
            .retain(|_, p| p.connection != connection);
    }
    pub(crate) fn cancel_confirmation(&self, caller: &Caller, id: &str) {
        let mut state = self.state.lock();
        if state
            .proposals
            .get(id)
            .is_some_and(|p| p.connection == caller.connection)
        {
            state.proposals.remove(id);
        }
    }
    pub fn audit(&self) -> Vec<Audit> {
        self.state.lock().audit.iter().cloned().collect()
    }
}

pub(super) fn patch(request: &Request) -> Result<(&Target, &str, &str, Vec<Edit>), Failure> {
    Ok(match request {
        Request::Set {
            target,
            store,
            path,
            value,
            expected_revision,
        } => (
            target,
            store,
            expected_revision,
            vec![Edit::Set {
                path: path.clone(),
                value: value.clone(),
            }],
        ),
        Request::Reset {
            target,
            store,
            path,
            expected_revision,
        } => (
            target,
            store,
            expected_revision,
            vec![Edit::Reset { path: path.clone() }],
        ),
        Request::Patch {
            target,
            store,
            expected_revision,
            operations,
        } => (target, store, expected_revision, operations.clone()),
        Request::Get { .. } => return Err(Error::new("settings", "invalid_type")),
    })
}
