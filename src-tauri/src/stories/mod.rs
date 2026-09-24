mod model;
mod store;

pub use model::*;
pub use store::StoryStore;

#[cfg(test)]
mod tests {
    use super::*;

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
}
