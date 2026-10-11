# Commit rollup capture

Recorded from GitHub GraphQL on 2026-10-11 with the GitHub CLI API recorder.
Repository: `curl/curl`, PR 23347, commit `6c5ad57264cc153ae583ed774d3f36b69992812f`.
`rollup-query.graphql` is the exact query. Variables: owner=curl, name=curl,
oid=the commit above; omit endCursor initially, then use the returned endCursor.
`rollup-curl-23347.json` contains the two unmodified response envelopes.

The capture includes failed Actions checks, a failed then successful `AM awslc`
check, and failed AppVeyor StatusContexts on the second page.

`rollup-no-checks.json` is a response recorded on the same date with the same
query, owner=sstraus, name=tuicommander and
oid=0b60f44e817a3239f9fcc021e618b5af2e360108. It pins GitHub's null rollup
shape for a commit without check data.
