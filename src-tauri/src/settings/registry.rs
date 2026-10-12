//! Routes a request to the one owner service registered for its store id.
//! Each adapter story adds one `register` call; the registry knows no store itself.
use super::{
    request::*,
    service::{Service, Store},
};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

type Outcome = Result<Reply, Failure>;

trait Owner: Send + Sync {
    fn execute(&self, request: Request) -> Pin<Box<dyn Future<Output = Outcome> + Send + '_>>;
}

impl<S: Store> Owner for Service<S> {
    fn execute(&self, request: Request) -> Pin<Box<dyn Future<Output = Outcome> + Send + '_>> {
        Box::pin(Service::execute(self, request))
    }
}

#[derive(Default)]
pub struct Registry {
    owners: BTreeMap<String, Arc<dyn Owner>>,
}

impl Registry {
    /// A store id has exactly one owner; a second registration is a wiring bug.
    pub fn register<S: Store>(&mut self, service: Service<S>) -> Result<(), Failure> {
        let id = service.store_id().to_owned();
        if self.owners.contains_key(&id) {
            return Err(Error::new(&id, "duplicate"));
        }
        self.owners.insert(id, Arc::new(service));
        Ok(())
    }

    pub async fn execute(&self, request: Request) -> Outcome {
        // The caller's string is never echoed: an unknown store reports under "settings".
        let owner = self
            .owners
            .get(request.store())
            .ok_or_else(|| Error::new("settings", "not_found"))?;
        owner.execute(request).await
    }
}
