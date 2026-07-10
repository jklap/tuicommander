# TODO — deferred improvements & ideas

Items surfaced during work that are larger than an in-line fix, need discussion,
or carry non-trivial risk. Immediate low-risk improvements are done in place, not
listed here.

## Open

### `get_github_remote_url` uses a loose `contains("github.com")` substring filter
- **Problem/opportunity:** `github.rs:2030` decides "is this a github.com repo?" via
  `url.contains("github.com")`. That is a substring check, so a remote like
  `https://github.company-internal.example/...` or any URL that merely contains the
  literal `github.com` anywhere passes. The security impact for the four parse sites is
  already mitigated (story 119-3150 added an `is_cloud()` gate in `parse_remote_url`, so a
  spoofed host now yields `None`), but the upstream filter itself is still imprecise.
- **Proposed solution:** Replace the substring test with host-aware parsing —
  reuse `github_account::parse_remote_url(url)` and check `host.is_cloud()` — so the
  filter and the parser share one host definition (single source of truth).
- **Expected benefits:** Removes the last loose host check; eliminates a class of
  false-positive "github.com" matches; one canonical host rule across the module.
- **Trade-offs:** `get_github_remote_url` becomes slightly heavier (full parse vs
  substring). Negligible — it already parses the URL immediately after.
- **Estimated complexity:** S (single function + a couple of unit tests).
- **Recommended priority:** P3 (correctness/DX polish; not a live vulnerability given the
  119 gate).
</content>
</invoke>
