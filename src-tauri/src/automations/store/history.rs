use super::*;

impl RunStore {
    /// Read newest-first pages, preserving history when definitions no longer exist.
    pub fn history(
        &self,
        automation_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AutomationRun>, String> {
        if !(1..=100).contains(&limit) {
            return Err("automation history limit must be between 1 and 100".into());
        }
        let conn = self.connect()?;
        let (sql, values): (&str, Vec<rusqlite::types::Value>) = match automation_id {
            Some(id) => (
                "SELECT snapshot_json FROM automation_runs WHERE automation_id=?1 ORDER BY created_ms DESC,id DESC LIMIT ?2 OFFSET ?3",
                vec![id.to_owned().into(), limit.into(), offset.into()],
            ),
            None => (
                "SELECT snapshot_json FROM automation_runs ORDER BY created_ms DESC,id DESC LIMIT ?1 OFFSET ?2",
                vec![limit.into(), offset.into()],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(error)?;
        stmt.query_map(rusqlite::params_from_iter(values), |r| {
            r.get::<_, String>(0)
        })
        .map_err(error)?
        .map(|row| decode(row.map_err(error)?))
        .collect()
    }

    /// Aggregate elapsed UTC windows [now-window, now], including open and skipped states.
    pub fn summary(&self, window: SummaryWindow, now_ms: i64) -> Result<RunSummary, String> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT status,COUNT(*) FROM automation_runs WHERE created_ms>=?1 AND created_ms<=?2 GROUP BY status").map_err(error)?;
        let mut summary = RunSummary::default();
        for row in stmt
            .query_map(
                params![now_ms.saturating_sub(window.millis()), now_ms],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .map_err(error)?
        {
            let (status, count) = row.map_err(error)?;
            let count = u64::try_from(count).map_err(error)?;
            let status =
                serde_json::from_value(serde_json::Value::String(status)).map_err(error)?;
            summary.total += count;
            summary.by_status.insert(status, count);
        }
        Ok(summary)
    }

    pub fn open_runs(&self) -> Result<Vec<AutomationRun>, String> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(&format!("SELECT snapshot_json FROM automation_runs WHERE status IN {OPEN} ORDER BY created_ms,id")).map_err(error)?;
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map_err(error)?
            .map(|row| decode(row.map_err(error)?))
            .collect()
    }

    /// Delete only final rows older than the cutoff; default retention is 90 days.
    pub fn prune(&self, now_ms: i64, retention_days: Option<u32>) -> Result<usize, String> {
        self.require_owner()?;
        let days = retention_days.unwrap_or(90);
        if days == 0 {
            return Err("automation retention must be positive".into());
        }
        let cutoff = now_ms.saturating_sub(i64::from(days) * SummaryWindow::Day.millis());
        self.connect()?
            .execute(
                &format!(
                    "DELETE FROM automation_runs WHERE status NOT IN {OPEN} AND finished_ms<?1"
                ),
                [cutoff],
            )
            .map_err(error)
    }
}
