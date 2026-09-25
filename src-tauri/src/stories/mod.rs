mod api;
mod model;
mod store;

pub use api::*;
pub use model::*;
pub use store::StoryStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_sets_schema_version_one() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");

        StoryStore::open_at(&db).expect("open store");

        let connection = rusqlite::Connection::open(db).expect("open database");
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, 1);
    }

    #[test]
    fn store_rejects_a_newer_schema_version() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let connection = rusqlite::Connection::open(&db).expect("open database");
        connection
            .pragma_update(None, "user_version", 2)
            .expect("set future schema version");

        let error = StoryStore::open_at(&db).expect_err("future schema is rejected");
        assert!(error.contains("newer than supported"), "{error}");
    }

    #[test]
    fn version_zero_store_is_upgraded_in_place_with_its_data() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Pre-version plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        // Builds before the schema was versioned wrote the same tables at user_version 0.
        rusqlite::Connection::open(&db)
            .and_then(|conn| conn.pragma_update(None, "user_version", 0))
            .expect("mark store as version 0");

        let reopened = StoryStore::open_at(&db).expect("version 0 opens");
        assert_eq!(
            reopened.get_plan(&plan.id).expect("plan survives").title,
            "Pre-version plan"
        );
        let version: i64 = rusqlite::Connection::open(&db)
            .expect("open database")
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, 1);
    }

    #[test]
    fn operator_can_work_a_story_without_a_terminal_claim() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Manual plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Manual work".into(),
                criteria: vec!["Verified".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        assert!(
            store
                .transition_for_actor(
                    &story.id,
                    story.revision,
                    StoryCommand::StartManual,
                    Some("agent")
                )
                .is_err()
        );
        let started = store
            .transition(&story.id, story.revision, StoryCommand::StartManual)
            .expect("start");
        assert_eq!(started.status, StoryStatus::InProgress);
        assert_eq!(started.claim_session, None);
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .expect("check");
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .expect("review");
        let done = store
            .transition(&story.id, review.revision, StoryCommand::Approve)
            .expect("approve");
        assert_eq!(done.status, StoryStatus::Done);
    }

    #[test]
    fn plan_and_story_survive_reopen_and_plan_state_is_derived() {
        let dir = tempfile::tempdir().expect("temporary config");
        let db = dir.path().join("stories.sqlite3");
        let store = StoryStore::open_at(&db).expect("open store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Ship feature".into(),
                source: "plans/feature.md".into(),
            })
            .expect("create plan");
        assert_eq!(store.plan_state(&plan.id).expect("state"), PlanState::Draft);
        let story = store
            .create_story(NewStory {
                plan_id: plan.id.clone(),
                title: "Implement core".into(),
                criteria: vec!["Core behavior works".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec!["src/core.rs".into()],
            })
            .expect("create story");
        assert_eq!(
            store.plan_state(&plan.id).expect("state"),
            PlanState::Active
        );
        drop(store);

        let reopened = StoryStore::open_at(&db).expect("reopen store");
        assert_eq!(reopened.get_story(&story.id).expect("story"), story);
        assert_eq!(
            reopened.plan_state(&plan.id).expect("state"),
            PlanState::Active
        );
        let claimed = reopened
            .claim(&story.id, "tab", story.revision)
            .expect("claim");
        let checked = reopened
            .transition(&story.id, claimed.revision, StoryCommand::CheckCriterion(0))
            .expect("check criterion");
        let review = reopened
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .expect("submit review");
        reopened
            .transition(&story.id, review.revision, StoryCommand::Approve)
            .expect("approve story");
        assert_eq!(
            reopened.plan_state(&plan.id).expect("state"),
            PlanState::Done
        );
    }

    #[test]
    fn dependency_cycle_and_stale_revision_are_rejected() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let make_story = |title: &str| NewStory {
            plan_id: plan.id.clone(),
            title: title.into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        };
        let a = store.create_story(make_story("A")).expect("A");
        let b = store.create_story(make_story("B")).expect("B");
        store
            .add_dependency(&b.id, &a.id, b.revision)
            .expect("B depends on A");
        assert!(store.add_dependency(&a.id, &b.id, a.revision).is_err());
        assert!(
            store
                .transition(&a.id, a.revision, StoryCommand::Approve)
                .is_err()
        );
        assert!(
            store
                .transition(&b.id, b.revision, StoryCommand::SubmitReview)
                .is_err()
        );
        assert_eq!(
            store.get_story(&b.id).expect("B").status,
            StoryStatus::Backlog
        );
        let a = store.claim(&a.id, "tab", a.revision).expect("claim A");
        let a = store
            .transition(&a.id, a.revision, StoryCommand::CheckCriterion(0))
            .expect("check A");
        let a = store
            .transition(&a.id, a.revision, StoryCommand::SubmitReview)
            .expect("review A");
        store
            .transition(&a.id, a.revision, StoryCommand::Approve)
            .expect("approve A");
        assert_eq!(
            store.get_story(&b.id).expect("B").status,
            StoryStatus::Ready
        );
    }

    #[test]
    fn manual_claim_conflicts_and_is_released_with_session() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "A".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        let claimed = store
            .claim(&story.id, "tab-one", story.revision)
            .expect("claim");
        assert!(
            store
                .transition_for_actor(
                    &story.id,
                    claimed.revision,
                    StoryCommand::CheckCriterion(0),
                    Some("tab-two"),
                )
                .is_err()
        );
        assert_eq!(
            store.get_story(&story.id).expect("story").revision,
            claimed.revision
        );
        assert!(store.claim(&story.id, "tab-two", claimed.revision).is_err());
        assert_eq!(store.release_session_claims("tab-one").expect("release"), 1);
        let released = store.get_story(&story.id).expect("story");
        assert!(released.claim_session.is_none());
        assert_eq!(released.status, StoryStatus::Ready);
        store
            .claim(&story.id, "tab-two", released.revision)
            .expect("second claim");
    }

    #[test]
    fn story_scope_cannot_escape_the_project_on_any_platform() {
        let dir = tempfile::tempdir().expect("temporary config");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        for scope in [
            "../secret",
            "/tmp/secret",
            "C:\\secret",
            "\\\\server\\share",
        ] {
            let result = store.create_story(NewStory {
                plan_id: plan.id.clone(),
                title: "Unsafe scope".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![scope.into()],
            });
            assert!(result.is_err(), "accepted unsafe scope: {scope}");
        }
    }

    #[test]
    fn closed_session_releases_claim_without_creating_an_unused_database() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let db = dir.path().join("stories.sqlite3");
        assert_eq!(
            StoryStore::release_closed_session("unused").expect("no store"),
            0
        );
        assert!(!db.exists());

        let store = StoryStore::open().expect("store");
        let plan = store
            .create_plan(NewPlan {
                project: "/project".into(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .expect("plan");
        let story = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Work".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .expect("story");
        store
            .claim(&story.id, "tab-one", story.revision)
            .expect("claim");
        assert_eq!(
            StoryStore::release_closed_session("tab-one").expect("release"),
            1
        );
        assert_eq!(
            store.get_story(&story.id).expect("story").status,
            StoryStatus::Ready
        );
    }

    #[test]
    fn closed_unclaimed_session_does_not_wait_for_a_story_write_lock() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let store = StoryStore::open().expect("store");
        let lock = rusqlite::Connection::open(dir.path().join("stories.sqlite3"))
            .expect("open locking connection");
        lock.execute_batch("BEGIN IMMEDIATE").expect("acquire write lock");

        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            sent.send(StoryStore::release_closed_session("unclaimed"))
                .expect("send release result");
        });
        assert_eq!(
            received
                .recv_timeout(std::time::Duration::from_millis(250))
                .expect("unclaimed release must not wait for a write transaction")
                .expect("release"),
            0
        );
        lock.execute_batch("ROLLBACK").expect("release write lock");
        drop(store);
    }

    #[test]
    fn closed_session_release_survives_a_store_without_its_schema() {
        let dir = tempfile::tempdir().expect("temporary config");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        // A file whose schema never landed (for example a crash between open and DDL):
        // the read-only probe cannot see a `stories` table, and teardown must not fail on it.
        rusqlite::Connection::open(dir.path().join("stories.sqlite3"))
            .and_then(|conn| conn.execute_batch("CREATE TABLE unrelated(x INTEGER)"))
            .expect("create schema-less store");
        assert_eq!(
            StoryStore::release_closed_session("unclaimed").expect("release"),
            0
        );
    }
}
