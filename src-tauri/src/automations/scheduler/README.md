# Scheduler admission

`scheduler::tick(store, config, now)` is deterministic. A future runtime calls it
once every 30 seconds; this module starts no thread and launches no agents.
It returns newly recorded decisions. Only `Reserved` decisions are dispatchable.

Each enabled definition evaluates its stored cron and zone once, selecting only
the latest occurrence at or before `now`. Its age may equal `grace_secs` and
still run. Older latest occurrences become `skipped_missed`; intervening
occurrences are covered by the cursor rather than materialized as a backlog.

SQLite `IMMEDIATE` admission transactions serialize scheduled and manual calls.
A durable per-definition high-water cursor, refusal/final record, overlap check
and capacity check commit before dispatch. Cursors survive retention and
restart, so clock rollback and repeated ticks cannot replay old occurrences.
Capacity defaults to the configuration's two open runs. Reserved, prechecking,
running and needs-you runs count; reducing the limit never cancels them.
Overlap takes precedence over capacity; missed occurrences take precedence over
both. Refused occurrences never queue for later execution.

`run_now` ignores paused state but observes overlap and capacity. Manual runs do
not move the scheduled cursor. Dispatch must skip prechecks for `Manual`
triggers. Boot recovery remains RunOwner's responsibility: every open run is
interrupted and never automatically retried.
