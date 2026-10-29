# Windows CI round g — story 2310-6e76

## Scope and evidence

Native Windows CI run `40724584147`, PR #148, tested main `bf904c1f03` and failed these 24 owners on all three attempts. Input evidence is `~/Gits/.tmp/results/ci-win-40724584147.log` and `ci-win-40724584147-panics.txt`. This branch starts from local main `287a11f49c` (including 2324), with start commit `cb54017a37`.

This is a new failure set. It does not replace the earlier 24-owner investigation in `reports/2310-windows-ci.md` or claim that original full-Windows acceptance is complete.

## Per-owner classification

Product means shipped behavior. Fixture includes test-only infrastructure. A mixed row names both defects; correcting the seed alone must not hide a real HTTP status or profile-decoding defect.

| # | Owner | Classification | Decisive code/log reason and repair |
|---|---|---|---|
| 1 | `git::tests::cached_worktree_inventory_tracks_storage_changes` | Product | The archive-pool adapter canonicalizes the repository before looking up its storage override. On Windows the canonical key has the verbatim prefix, but the persisted fixture/registered key does not. Exact-only lookup drops the Sibling override. The fix compares existing keys through portable_spelling after the exact match. |
| 2 | `mcp_http::tests::publishing_an_unchanged_draft_is_a_client_error` | Product + fixture | The test seeds WorkflowStore under the raw temporary path while definition_action verifies the canonical owner. Seed with the API owner. Independently, a real cross-project workflow request is a caller error but the HTTP classifier reports 500; a new real-route regression reproduces it on rb. |
| 3 | `mcp_http::tests::worktree_remove_by_path_preserves_the_registration_of_a_vanished_checkout` | Fixture | Git porcelain prints portable paths; the final registration assertion searches for the raw native path. Compare the same path in Git spelling; retain all refusal/repair assertions. |
| 4 | `monitoring_status::tests::monitoring_status_inventory_add_after_main_entry_eviction_keeps_sibling_reusable` | Fixture | The monitoring test helper passes canonical verbatim Windows destination/source arguments to Git worktree add or clone. The helper now uses the existing portable_spelling contract at its Git argv boundary. No production cache logic, assertion or deadline changes. |
| 5 | `monitoring_status::tests::monitoring_status_inventory_add_keeps_unchanged_sibling_reusable` | Fixture | The monitoring test helper passes canonical verbatim Windows destination/source arguments to Git worktree add or clone. The helper now uses the existing portable_spelling contract at its Git argv boundary. No production cache logic, assertion or deadline changes. |
| 6 | `monitoring_status::tests::monitoring_status_rejects_unproven_linked_admin_mapping` | Fixture | The monitoring test helper passes canonical verbatim Windows destination/source arguments to Git worktree add or clone. The helper now uses the existing portable_spelling contract at its Git argv boundary. No production cache logic, assertion or deadline changes. |
| 7 | `monitoring_status::tests::monitoring_status_snapshot_keeps_unchanged_sibling_out_of_refresh` | Fixture | The monitoring test helper passes canonical verbatim Windows destination/source arguments to Git worktree add or clone. The helper now uses the existing portable_spelling contract at its Git argv boundary. No production cache logic, assertion or deadline changes. |
| 8 | `monitoring_status::tests::monitoring_status_watch_loss_and_expiry_force_actual_fresh_scans` | Fixture | The monitoring test helper passes canonical verbatim Windows destination/source arguments to Git worktree add or clone. The helper now uses the existing portable_spelling contract at its Git argv boundary. No production cache logic, assertion or deadline changes. |
| 9 | `remote_runtime::relay_tests::desktop_attaches_through_the_relay_and_runs_a_command` | Fixture | The real relay and daemon are reached, but the fixture submits POSIX arithmetic to the Windows default shell. Explicitly request host_shell and use its shared command spelling; only evaluated output contains 42relay. Keep the 60-second bound. |
| 10 | `repo_watcher::tests::a_commit_reports_git_state_not_working_tree_even_after_preceding_edits` | Fixture | The test passes a canonical verbatim Windows destination directly to git worktree add. Only its destination argv is converted to portable_spelling; watcher state and assertions are unchanged. |
| 11 | `session_resume::tests::resume_preserves_agent_selectors_and_custom_binaries` | Fixture | The implementation uses host shell quoting, but the oracle requires Unix single quotes for every agent. Keep the exact selector/id check with host quoting. |
| 12 | `session_resume::tests::resume_verifies_the_launch_profile_and_preserves_quoted_environment` | Product + fixture | unquote consumes every backslash outside single quotes, corrupting the double-quoted Windows CLAUDE_CONFIG_DIR before verify_agent_session reads it. The host-aware decoder preserves Windows backslashes and decodes launch-writer quotes/carets. The later command oracle also needs host quoting. |
| 13 | `settings::policy_scope_tests::execution_class_requires_confirmation` | Fixture | /verification/ego is rooted but not absolute on Windows, so validation correctly returns out_of_range before policy confirmation. Supply an actual host-absolute scratch-root child, retaining both confirmation and relative-path refusal assertions. |
| 14 | `stories::api::tests::archive_round_trip_checks_revisions_and_preserves_history_and_contents` | Fixture | story_action writes the canonical project owner, while direct StoryStore reads use the raw temp path. Resolve the owner before the API/direct-store round trip; retain exact history/revision/content assertions. |
| 15 | `stories::api::tests::new_plan_archived_source_has_visible_result` | Fixture | The direct archive-list read uses the raw project key although AddPlanSource stores the canonical owner. Resolve the same owner before seeding/reading; retain exact archived/restored record assertions. |
| 16 | `stories::list_tests::list_stories_filters_by_state_and_priority` | Fixture | with_plan seeds StoryStore with the raw temp path, but story_action verifies the canonical project owner. Resolve that owner once before store seeding and API calls; keep all filtering, paging, cursor and wire assertions. |
| 17 | `stories::list_tests::list_stories_pages_filtered_ids_without_capping_legacy_reads_or_losing_wire_shape` | Fixture | with_plan seeds StoryStore with the raw temp path, but story_action verifies the canonical project owner. Resolve that owner once before store seeding and API calls; keep all filtering, paging, cursor and wire assertions. |
| 18 | `stories::list_tests::list_stories_rejects_invalid_bounds_and_cursors_from_another_plan_or_query` | Fixture | with_plan seeds StoryStore with the raw temp path, but story_action verifies the canonical project owner. Resolve that owner once before store seeding and API calls; keep all filtering, paging, cursor and wire assertions. |
| 19 | `stories::note_tests::story_note_receipts_reference_real_persisted_events_in_the_same_plan` | Fixture | with_story seeds the plan under the raw temp path, but note actions and workflow receipt validation use the canonical owner. Resolve the owner before seeding; preserve note content, history, revisions, receipts and cross-project refusal assertions. |
| 20 | `stories::note_tests::story_notes_preserve_text_at_the_unicode_character_limit` | Fixture | with_story seeds the plan under the raw temp path, but note actions and workflow receipt validation use the canonical owner. Resolve the owner before seeding; preserve note content, history, revisions, receipts and cross-project refusal assertions. |
| 21 | `stories::note_tests::story_notes_reject_stale_invalid_or_cross_project_requests_without_appending` | Fixture | with_story seeds the plan under the raw temp path, but note actions and workflow receipt validation use the canonical owner. Resolve the owner before seeding; preserve note content, history, revisions, receipts and cross-project refusal assertions. |
| 22 | `stories::note_tests::story_notes_survive_reopen_in_append_order_without_rewriting_subject_revisions` | Fixture | with_story seeds the plan under the raw temp path, but note actions and workflow receipt validation use the canonical owner. Resolve the owner before seeding; preserve note content, history, revisions, receipts and cross-project refusal assertions. |
| 23 | `tests::test_process_removes_its_isolated_gitconfig_on_exit` | Fixture infrastructure | The isolated test-process cleanup relies on ctor::dtor/CRT atexit. Rust Windows sys/exit.rs calls ExitProcess, which does not take that CRT path. Add a process-detach TLS callback that uses the retained owner PID and pre-encoded absolute filename with DeleteFileW, without allocation or Rust locks. Existing return/exit/override assertions remain intact; native linker/runtime proof is owed to Windows CI. |
| 24 | `worktree::tests::sidebar_removal_identity_2334_candidate_states` | Fixture | Gix inventory exposes portable paths; the test searches for a raw canonical verbatim Path. Convert only the oracle spelling and retain every candidate-state/removal identity assertion. |

## Regression evidence and limits

- Profile parser RED: `shell_launch_preserves_backslashes_in_double_quoted_profile_paths`, pre-fix `cb54017a37` source plus the regression. rb Linux decoded `D:\profiles\private profile` as `D:profilesprivate profile`; 1 failed, summary 0.043s, build 3m24s. Same call: Clippy for tuicommander, tuic-git and tuic-test-support, all targets, `-D warnings`, passed in 1m23s. Log: `~/Gits/.tmp/results/tuic-2310g-red-rb-v3.log`.
- HTTP scope RED: `workflow_project_scope_refusal_is_a_client_error`, pre-fix `cb54017a37` workflow classifier plus the regression and other round-g repairs. Real canonical-owner seeding and a second real project returned HTTP 500, expected 422. rb Linux: 1 failed, summary 0.179s, build 2m17s. Same three-crate Clippy call passed in 1m00s. Log: `~/Gits/.tmp/results/tuic-2310g-scope-red-rb.log`.
- Storage override RED is the existing native Windows `cached_worktree_inventory_tracks_storage_changes` failure on `bf904c1f03`, at its positive Sibling-override assertion. It uses real Git and persisted RepoSettings, not a simulated path label.
- Gitconfig cleanup RED is the existing native Windows `test_process_removes_its_isolated_gitconfig_on_exit` failure on `bf904c1f03`, specifically `exit`, not `return`. Local installed Rust 1.99.1 source `library/std/src/sys/exit.rs` selects `ExitProcess` for Windows. dtor 0.3.0 source registers CRT `atexit`; native post-edit callback/linker execution remains unverified.
- Initial RED launch selected system openrsync and exited 69 before execution; corrected PATH selects Homebrew rsync. The next build exposed main's duplicate `event_refusals_are_durable_coalesced_and_do_not_consume_admission` function (E0428). For the two independent REDs only, the second copy was temporarily renamed, with no test removed or skipped. The file is restored byte-for-byte; coordinator owns story 2454 and its main landing. No change to that file is committed here.
- No post-fix GREEN run, full suite, Windows execution, desktop launch, push, deadline change, skip or assertion weakening is claimed. The coordinator owns native Windows CI and the integration GREEN.

## Adjacent scenarios and worst case

- Parser: existing seed-brief removal/custom-binary cases remain. The added native-writer round-trip table covers UNC/trailing backslash, spaces, apostrophe, quote and cmd metacharacters. This table is not executed in the implementation lane; it is part of the integration filter.
- Storage: exact-key precedence remains; only the fallback scans portable keys. Missing overrides still use defaults. Existing owner test checks InsideRepo, Sibling, no-op saves and removal of the override. Worst case is one linear scan of the persisted repository map per cold pool resolution, with no new filesystem/process work.
- HTTP: valid same-project publication still uses the unchanged-draft 422 test. Cross-project reads still fail, but now as 422. Validation prefix and unchanged-publish classification remain intact. No payload/route or authorization change.
- Cleanup: return, explicit exit and environment override retain the original assertions. The callback checks the recorded owner PID and does not read mutable environment. It performs at most one direct DeleteFileW attempt per process detach, with a NUL-terminated path encoded at construction; the normal CRT finalizer can harmlessly attempt the same removal. Abort/forced termination cleanup remains outside the contract. No external cleanup process is created.
- Relay: the session explicitly uses host_shell. The submitted command never contains `42relay`; only shell evaluation emits it. Existing relay pairing, endpoint, real PTY and exact marker checks remain; WAIT stays 60s.

## Integration command

Run through `build-slot.sh --remote` for Linux, or through the coordinator's native Windows CI workflow. Tests must retain repository setup scripts/with-test-tmp and default features/profile. Native Windows is required for the process-detach linker path and native Git behavior.

```sh
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuicommander -p tuic-git -p tuic-test-support --lib -E '
test(cached_worktree_inventory_tracks_storage_changes) |
test(publishing_an_unchanged_draft_is_a_client_error) |
test(worktree_remove_by_path_preserves_the_registration_of_a_vanished_checkout) |
test(monitoring_status_inventory_add_after_main_entry_eviction_keeps_sibling_reusable) |
test(monitoring_status_inventory_add_keeps_unchanged_sibling_reusable) |
test(monitoring_status_rejects_unproven_linked_admin_mapping) |
test(monitoring_status_snapshot_keeps_unchanged_sibling_out_of_refresh) |
test(monitoring_status_watch_loss_and_expiry_force_actual_fresh_scans) |
test(desktop_attaches_through_the_relay_and_runs_a_command) |
test(a_commit_reports_git_state_not_working_tree_even_after_preceding_edits) |
test(resume_preserves_agent_selectors_and_custom_binaries) |
test(resume_verifies_the_launch_profile_and_preserves_quoted_environment) |
test(execution_class_requires_confirmation) |
test(archive_round_trip_checks_revisions_and_preserves_history_and_contents) |
test(new_plan_archived_source_has_visible_result) |
test(list_stories_filters_by_state_and_priority) |
test(list_stories_pages_filtered_ids_without_capping_legacy_reads_or_losing_wire_shape) |
test(list_stories_rejects_invalid_bounds_and_cursors_from_another_plan_or_query) |
test(story_note_receipts_reference_real_persisted_events_in_the_same_plan) |
test(story_notes_preserve_text_at_the_unicode_character_limit) |
test(story_notes_reject_stale_invalid_or_cross_project_requests_without_appending) |
test(story_notes_survive_reopen_in_append_order_without_rewriting_subject_revisions) |
test(test_process_removes_its_isolated_gitconfig_on_exit) |
test(sidebar_removal_identity_2334_candidate_states) |
test(shell_launch_preserves_backslashes_in_double_quoted_profile_paths) |
test(shell_launch_round_trips_native_quoted_profile_values) |
test(workflow_project_scope_refusal_is_a_client_error) |
test(definition_actions_are_scoped_to_the_canonical_owner)'
```
