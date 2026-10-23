// One integration-test target per crate (story 2059-5902,
// docs/evidence/integration-test-targets-2059.md): each former
// `tests/<name>.rs` target is a module under `tests/cases/`.
#[path = "cases/identity_probe.rs"]
mod identity_probe;
#[path = "cases/socket_environment.rs"]
mod socket_environment;
#[path = "cases/socket_root.rs"]
mod socket_root;
#[path = "cases/temp_roots.rs"]
mod temp_roots;
