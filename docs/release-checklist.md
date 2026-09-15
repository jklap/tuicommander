# Release & Tag Checklist

When Boss asks to tag a release:

1. **Update version**: run `make bump V=x.y.z` (updates all manifests, CHANGELOG, SPEC.md, and generates AI release notes with contributor extraction via `scripts/generate-release-notes.sh`)
2. **Review release notes** — the script shows AI-generated notes for approval (Y/edit/regenerate/quit). Ensure `### Community` section in CHANGELOG lists all external contributors with PR links
3. **Commit** with message `chore: bump version to vX.Y.Z`
4. **Tag** with `git tag vX.Y.Z`
5. **Publish the tag** — push the approved commit and tag through the normal release procedure. The `Release` workflow builds all platforms and the remote daemon, uploads signed installers and `latest.json`, then publishes the draft with changelog notes. Wait for its successful conclusion and verify the assets; do not create a second release with `gh release create`
6. **Milestone** — close the matching milestone if one exists, create the next one
7. **macOS Finder Service** — after a packaged `.dmg`/`.app` build, confirm the "New TUICommander
   Tab Here" Finder integration still installs and works: install it from Settings → General,
   right-click a folder in Finder, and confirm the pane opens. The bundle is hand-authored
   (`src-tauri/services/`), not machine-generated, so a packaging change (resource paths,
   `externalBin` layout) can silently break it without any build error.

## Prereleases (betas)

Use `make bump V=2.0.0-beta.1` and tag `v2.0.0-beta.1`. `scripts/release-flags.sh` makes `release.yml` publish any version with a `-` suffix as a prerelease that is never "latest", and `notify-tap.yml` skips prereleases. The stable updater endpoint (`releases/latest/download/latest.json`) and the install scripts read "latest", so stable users stay on stable. After every completed versioned beta release, the finalization job copies its signed `latest.json` to the rolling `beta` prerelease (never latest). Installers remain on the immutable versioned release. Settings → General → Expert Mode → Update Channel → Beta checks this feed automatically and also checks stable, so testers receive the next beta and the final release. New beta installations default to Beta; an existing saved channel is preserved, so existing users who install a beta must choose Beta once. Beta updates open the versioned download page; final stable updates install in-app. Nightly behavior is unchanged. An older beta finishing later cannot replace a newer feed. No beta is published if the versioned release or its manifest is incomplete.

The macOS terminal installer preserves an installed Beta or Nightly channel when no version is given. Use `install.sh beta` or `install.sh nightly` to select one explicitly, or pass a version with or without a single `v` prefix to pin it. A default stable install resolves GitHub's latest stable tag. Beta downloads use the immutable tag named by the rolling manifest.

## GitHub Issue Management

- **Labels**: Use `type:`, `P0-P3:`, `area:`, `effort:` prefixes. Apply `needs triage` to new issues.
- **Milestones**: Assign issues to version milestones (v0.4.0, v1.0.0, etc.)
- **Issue templates**: Bug reports and feature requests use `.github/ISSUE_TEMPLATE/*.yml` forms
- **Token for project ops**: Use `GH_TOKEN=$GH_STRAUS gh ...` when commands need the `project` scope (the default `gh auth` token only has `repo` + `workflow`)
