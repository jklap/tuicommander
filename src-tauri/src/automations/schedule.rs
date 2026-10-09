//! Zone-aware cron semantics for scheduled automations.

#[cfg(test)]
mod tests {
    use crate::automations::model::AutomationDefinition;
    use serde_json::json;

    // Catches: malformed cron being persisted as an executable definition.
    #[test]
    fn malformed_cron_is_rejected_before_storage() {
        let definition: AutomationDefinition = serde_json::from_value(json!({
            "id":"invalid", "name":"Invalid cron", "prompt":"Review",
            "run_config":"codex", "repository":"/project",
            "workspace":{"mode":"existing"}, "cron":"not a cron",
            "timezone":"America/New_York", "enabled":true,
            "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600
        })).unwrap();
        assert!(definition.validate().is_err(), "malformed cron was accepted");
    }
}
