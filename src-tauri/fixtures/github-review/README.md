# GitHub review API fixtures

Recorded read-only from GitHub on 2026-10-10 with authenticated `gh api`.
No mutation was sent to any repository.

- `threads-recorded.json`: GraphQL `repository(owner:"microsoft", name:"vscode")` → `pullRequests(last:10, states:MERGED)`, selecting `number`, `headRefOid`, and `reviewThreads(first:10)`. Thread selection: id, path, line, originalLine, diffSide, isResolved, isOutdated, comments(first:100) with pageInfo and id/databaseId/body/url/author.login/createdAt. Text and author handles are scrubbed. Includes outdated threads with null current lines.
- `reviews-recorded.json`: REST `GET repos/microsoft/vscode/pulls/340803/reviews`. Only id, html_url, state, commit_id are retained.
- `schema-recorded.json`, `reply-schema-recorded.json`: GraphQL introspection of thread/reply/queue fields and input types. The reply review id is optional; thread id and body are required.

Tests run against a local API stub. Read responses use these recorded values. Mutation response envelopes are the documented GitHub GraphQL payload shapes, containing recorded nodes modified by the local stub; these are not recordings of live mutations. Local write capture verifies REST coordinates and GraphQL input without external writes.

Contracts: [review creation](https://docs.github.com/en/rest/pulls/reviews#create-a-review-for-a-pull-request), [pull request GraphQL reference](https://docs.github.com/en/graphql/reference/pull-requests).
