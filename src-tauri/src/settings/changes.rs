//! Value-free invalidation cursor. CAS revisions remain owned by each store.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SettingsChanged {
    pub section: String,
    pub generation: u64,
    pub persisted: bool,
    pub applied: bool,
}
