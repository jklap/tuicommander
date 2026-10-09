//! The scheduled cursor and admission decision commit together before dispatch.
use super::*;

impl RunStore {
    /// Reserve or durably refuse work. A scheduled occurrence is never queued.
    pub(in crate::automations) fn admit(
        &self,
        definition: &AutomationDefinition,
        trigger: RunTrigger,
        max_concurrent_runs: u32,
        now_ms: i64,
    ) -> Result<Option<AutomationRun>, String> {
        self.require_owner()?;
        definition.validate()?;
        if max_concurrent_runs == 0 {
            return Err("Automation concurrency must be greater than zero".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let mut missed = false;
        if let RunTrigger::Scheduled { occurrence_ms } = trigger {
            if !definition.enabled {
                return Ok(None);
            }
            if occurrence_ms > now_ms {
                return Err("Automation occurrence is in the future".into());
            }
            // Include reservations predating the scheduler; manual history never moves this cursor.
            let cursor: Option<i64> = tx.query_row(
                "SELECT MAX(occurrence_ms) FROM (SELECT occurrence_ms FROM automation_cursors WHERE automation_id=?1 UNION ALL SELECT MAX(occurrence_ms) AS occurrence_ms FROM automation_runs WHERE automation_id=?1)",
                [&definition.id], |r| r.get(0),
            ).map_err(error)?;
            if cursor.is_some_and(|cursor| occurrence_ms <= cursor) {
                return Ok(None);
            }
            tx.execute("INSERT INTO automation_cursors(automation_id,occurrence_ms) VALUES(?1,?2) ON CONFLICT(automation_id) DO UPDATE SET occurrence_ms=excluded.occurrence_ms", params![definition.id,occurrence_ms]).map_err(error)?;
            missed = i128::from(now_ms) - i128::from(occurrence_ms)
                > i128::from(definition.grace_secs) * 1000;
        }
        let initial_status = if missed {
            RunStatus::SkippedMissed
        } else {
            let overlap: bool = tx.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM automation_runs WHERE automation_id=?1 AND status IN {OPEN})"),
                [&definition.id], |r| r.get(0),
            ).map_err(error)?;
            if overlap {
                RunStatus::SkippedOverlap
            } else {
                let open: i64 = tx
                    .query_row(
                        &format!("SELECT COUNT(*) FROM automation_runs WHERE status IN {OPEN}"),
                        [],
                        |r| r.get(0),
                    )
                    .map_err(error)?;
                if open >= i64::from(max_concurrent_runs) {
                    RunStatus::SkippedConcurrency
                } else {
                    RunStatus::Reserved
                }
            }
        };
        let run = reserve_in(&tx, definition, trigger, initial_status, now_ms)?;
        tx.commit().map_err(error)?;
        Ok(run)
    }
}
