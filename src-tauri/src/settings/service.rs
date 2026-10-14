//! Internal owner service; no transport or origin/confirmation authority is granted here.
use super::{
    catalogue::Catalogue,
    model::*,
    pointer,
    request::*,
    transaction::{self, Prepared},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, sync::Arc};

/// An adapter owns strict reads, validation, locking, persistence and runtime effects.
/// `commit` MUST invoke `prepare` on latest state under the same owner/file lock as
/// its one atomic write. It must not call `apply` or publish before persistence.
pub trait Store: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn target(&self) -> Target;
    /// Bind a request to its owner without changing fixed-target adapters.
    fn resolve(self: Arc<Self>, target: &Target) -> Result<Arc<Self>, Failure> {
        if *target != self.target() {
            return Err(Error::new(self.id(), "not_found"));
        }
        Ok(self)
    }
    fn read(&self) -> Result<Value, Failure>;
    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure>;
    /// `false` means effects were scheduled but completion still needs readback.
    fn apply(&self, reply: &CommitReply) -> impl Future<Output = Result<bool, Failure>> + Send;
}

pub struct Service<S> {
    catalogue: Arc<Catalogue>,
    owner: Arc<S>,
    policy: Arc<super::policy::Policy>,
}

impl<S: Store> Service<S> {
    pub fn new(catalogue: Arc<Catalogue>, owner: Arc<S>) -> Self {
        Self {
            catalogue,
            owner,
            policy: Arc::new(super::policy::Policy::default()),
        }
    }

    /// Share the host proposal pool with the real all-client confirmation path.
    pub fn with_policy(mut self, policy: Arc<super::policy::Policy>) -> Self {
        self.policy = policy;
        self
    }

    /// Host-issued caller context is separate from the model's request payload.
    pub async fn execute_as(
        &self,
        request: Request,
        _caller: &super::policy::Caller,
        _approval: Option<&str>,
    ) -> Result<Reply, Failure> {
        self.execute(request).await
    }

    pub fn store_id(&self) -> &str {
        self.owner.id()
    }

    fn check_owner(&self, store: &str) -> Result<(), Failure> {
        if store != self.owner.id() {
            // Use the registered store, never echo an arbitrary caller string.
            return Err(Error::new(self.owner.id(), "not_found"));
        }
        if !self.catalogue.fields.values().any(|f| f.store == store) {
            return Err(Error::new(self.owner.id(), "unavailable"));
        }
        Ok(())
    }

    pub async fn execute(&self, request: Request) -> Result<Reply, Failure> {
        match request {
            Request::Get {
                target,
                store,
                paths,
            } => self.get(target, store, paths).await.map(Reply::Snapshot),
            Request::Set {
                target,
                store,
                path,
                value,
                expected_revision,
            } => self
                .patch(
                    target,
                    store,
                    expected_revision,
                    vec![Edit::Set { path, value }],
                )
                .await
                .map(Reply::Commit),
            Request::Reset {
                target,
                store,
                path,
                expected_revision,
            } => self
                .patch(target, store, expected_revision, vec![Edit::Reset { path }])
                .await
                .map(Reply::Commit),
            Request::Patch {
                target,
                store,
                expected_revision,
                operations,
            } => self
                .patch(target, store, expected_revision, operations)
                .await
                .map(Reply::Commit),
        }
    }

    pub async fn get(
        &self,
        target: Target,
        store: String,
        paths: Vec<String>,
    ) -> Result<Snapshot, Failure> {
        self.check_owner(&store)?;
        if paths.is_empty() || paths.len() > 128 {
            let mut error = Error::new(&store, "out_of_range");
            error.expected = Some("paths array".into());
            error.allowed = vec!["minItems: 1".into(), "maxItems: 128".into()];
            return Err(error);
        }
        let cat = self.catalogue.clone();
        let owner = self.owner.clone();
        let safe_store = store.clone();
        tokio::task::spawn_blocking(move || {
            let owner = owner.resolve(&target)?;
            let mut latest = owner.read()?;
            let revision = transaction::revision(&latest)?;
            let mut values = BTreeMap::new();
            for path in paths {
                let parts = pointer::segments(&path, &store)?;
                let field = pointer::field(&cat, &store, &parts)?;
                if !matches!(field.availability, Availability::Available)
                    || !field.annotation.operations.contains(&Operation::Get)
                {
                    return Err(Error::new(&store, "unavailable"));
                }
                let value =
                    match pointer::at_mut(&mut latest, &parts, &cat, &store, &mut Vec::new()) {
                        Ok(value) => value.clone(),
                        Err(error) if field.nullable && error.code == "not_found" => {
                            let (key, parent) = parts.split_last().expect("nonempty pointer");
                            let parent = pointer::at_mut(
                                &mut latest,
                                parent,
                                &cat,
                                &store,
                                &mut Vec::new(),
                            )?;
                            if field.path.ends_with('}')
                                || !parent.is_object()
                                || parent.get(key).is_some()
                            {
                                return Err(error);
                            }
                            Value::Null
                        }
                        Err(error) => return Err(error),
                    };
                let stored = if matches!(
                    field.annotation.sensitivity,
                    Sensitivity::Secret | Sensitivity::DerivedSecret
                ) {
                    let set = !value.is_null() && value != "";
                    json!({"status": if set { "set" } else { "unset" }})
                } else {
                    value
                };
                values.insert(
                    path,
                    StoredValue {
                        effective: stored.clone(),
                        stored,
                        source: "stored".into(),
                    },
                );
            }
            Ok(Snapshot { revision, values })
        })
        .await
        .map_err(|_| Error::new(&safe_store, "store_failed"))?
    }

    pub async fn patch(
        &self,
        target: Target,
        store: String,
        expected_revision: String,
        operations: Vec<Edit>,
    ) -> Result<CommitReply, Failure> {
        self.check_owner(&store)?;
        let owner = self.owner.clone();
        let cat = self.catalogue.clone();
        let safe_store = store.clone();
        let (mut reply, owner) = tokio::task::spawn_blocking(move || {
            let owner = owner.resolve(&target)?;
            owner
                .commit(&mut |latest| {
                    transaction::prepare(&cat, &store, latest, &expected_revision, &operations)
                })
                .map(|reply| (reply, owner))
        })
        .await
        .map_err(|_| Error::new(&safe_store, "store_failed"))??;
        if !reply.changed_ids.is_empty() {
            match owner.apply(&reply).await {
                Ok(applied) => reply.applied = applied,
                Err(_) => {
                    let mut error = Error::new(&safe_store, "apply_failed");
                    error.persisted = reply.persisted;
                    error.revision = Some(reply.revision);
                    error.next_action = "get".into();
                    // Reread/recover effects; never invite replay of a durable mutation.
                    return Err(error);
                }
            }
        } else {
            reply.applied = true;
        }
        Ok(reply)
    }
}
