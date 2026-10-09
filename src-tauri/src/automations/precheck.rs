//! Bounded pre-dispatch checks. Run persistence belongs to the dispatcher.
use super::model::Precheck;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum PrecheckOutcome { NotConfigured, Bypassed, Executed(PrecheckResult) }
#[derive(Debug, PartialEq)]
pub struct PrecheckResult {
    pub termination: Termination,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}
#[derive(Debug, PartialEq)]
pub enum Termination { Exited(Option<i32>), TimedOut, SpawnError(String), ProcessError(String) }
impl PrecheckOutcome {
    pub fn proceeds(&self) -> bool {
        matches!(self, Self::NotConfigured | Self::Bypassed | Self::Executed(PrecheckResult { termination: Termination::Exited(Some(0)), .. }))
    }
}
pub async fn run_precheck(_workspace: &Path, _precheck: Option<&Precheck>, _manual: bool) -> PrecheckOutcome {
    PrecheckOutcome::Executed(PrecheckResult { termination: Termination::SpawnError("Precheck execution is not implemented".into()), stdout: String::new(), stderr: String::new(), stdout_truncated: false, stderr_truncated: false, duration_ms: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: a successful real shell check being refused instead of admitting dispatch.
    #[tokio::test]
    async fn zero_exit_admits_dispatch() {
        let precheck = Precheck { command: "echo ready".into(), timeout_secs: 30 };
        let outcome = run_precheck(&crate::test_support::test_temp_root(), Some(&precheck), false).await;
        assert!(outcome.proceeds(), "successful check must proceed: {outcome:?}");
    }
}
