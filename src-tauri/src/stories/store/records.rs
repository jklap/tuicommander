use super::*;
use rusqlite::{Transaction, TransactionBehavior};

pub(super) fn immediate(conn: &mut Connection) -> Result<Transaction<'_>, String> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("begin story transaction: {e}"))
}

pub(super) fn validate_text(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value.chars().count() > max {
        return Err(format!("{label} exceeds {max} characters"));
    }
    Ok(())
}

pub(super) fn validate_scope_path(path: &str) -> Result<(), String> {
    validate_text("file scope path", path, 4096)?;
    if crate::fs::is_absolute_on_any_platform(path)
        || path.contains(':')
        || path.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("file scope must remain relative to the project".into());
    }
    Ok(())
}

pub(super) fn encode(story: &Story) -> Result<String, String> {
    serde_json::to_string(story).map_err(|e| format!("encode story: {e}"))
}

pub(super) fn read_story(conn: &Connection, id: &str) -> Result<Story, String> {
    let document: String = conn
        .query_row("SELECT document FROM stories WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| format!("read story: {e}"))?
        .ok_or_else(|| format!("story not found: {id}"))?;
    serde_json::from_str(&document).map_err(|e| format!("decode story: {e}"))
}

pub(super) fn read_plan_stories(conn: &Connection, plan_id: &str) -> Result<Vec<Story>, String> {
    let mut stmt = conn
        .prepare("SELECT document FROM stories WHERE plan_id=?1 ORDER BY id")
        .map_err(|e| format!("list stories: {e}"))?;
    let documents = stmt
        .query_map([plan_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("list stories: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read stories: {e}"))?;
    documents
        .iter()
        .map(|doc| serde_json::from_str(doc).map_err(|e| format!("decode story: {e}")))
        .collect()
}

pub(super) fn check_revision(story: &Story, expected: i64) -> Result<(), String> {
    if story.revision != expected {
        return Err(format!(
            "stale story revision: expected {expected}, current {}",
            story.revision
        ));
    }
    Ok(())
}

pub(super) fn save_story(
    tx: &Transaction<'_>,
    story: &mut Story,
    expected: i64,
) -> Result<(), String> {
    story.revision = expected.checked_add(1).ok_or("story revision exhausted")?;
    let updated = tx.execute(
        "UPDATE stories SET document=?1,status=?2,revision=?3,claim_session=?4 WHERE id=?5 AND revision=?6",
        params![encode(story)?, story.status.as_str(), story.revision, story.claim_session, story.id, expected],
    ).map_err(|e| format!("save story: {e}"))?;
    if updated != 1 {
        return Err("stale story revision".into());
    }
    Ok(())
}

pub(super) fn dependencies_done(conn: &Connection, story: &Story) -> Result<bool, String> {
    for id in &story.dependencies {
        if read_story(conn, id)?.status != StoryStatus::Done {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn promote_ready(tx: &Transaction<'_>, plan_id: &str) -> Result<(), String> {
    for mut candidate in read_plan_stories(tx, plan_id)? {
        if candidate.status == StoryStatus::Backlog && dependencies_done(tx, &candidate)? {
            let revision = candidate.revision;
            candidate.status = StoryStatus::Ready;
            save_story(tx, &mut candidate, revision)?;
        }
    }
    Ok(())
}
