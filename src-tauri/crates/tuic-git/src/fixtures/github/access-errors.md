# GitHub partial-error fixture

Recorded on 2026-10-22 through the already authenticated `gh api graphql` command.
No credential was read, printed, copied or stored. Response bytes are in
`access-errors.json`.

```graphql
query AccessReasonFixture {
  missing: repository(owner:"stefanostraus",name:"caplock") { name }
  restricted: organization(login:"Lansweeper") { auditLog(first:1) { totalCount } }
}
```

The repository returns NOT_FOUND. The organization audit log returns FORBIDDEN
with a nested alias path. This proves the partial-error wire shape, not that
TUIC's OAuth app is blocked by organization policy. The latter still needs a
login in the isolated agent-test instance and a repository poll.

GitHub documents that NOT_FOUND/404 can also hide inaccessible resources; do not
claim it proves deletion. GitHub also documents the `X-GitHub-SSO` authorization
URL and that app-generated access tokens are automatically SAML-authorized:
https://docs.github.com/en/rest/authentication/authenticating-to-the-rest-api

DEFERRED (2026-10-22) — a real OAuth-restriction/SAML repository response cannot
be recorded without Boss authorizing TUIC's isolated test instance. Do not label
the recorded audit-log denial as either of those policy reasons.
