---
name: release-workspace
description: >
  Run the multi-crate workspace release process for metrics-rs/metrics: audit
  unreleased changes, plan version bumps and changelog entries, then execute
  per-crate (cargo-release publish + per-PR comments + S-awaiting-release
  label removal) with explicit gates between crates. Invoke with
  /release-workspace when the user asks to prepare/run a release, says "what's
  ready to release?", or wants to drain the S-awaiting-release queue.
---

# Workspace Release

Coordinated patch/minor releases of the workspace crates: `metrics`,
`metrics-util`, `metrics-exporter-prometheus`, `metrics-exporter-dogstatsd`,
`metrics-exporter-tcp`, `metrics-observer`, `metrics-tracing-context`,
`metrics-benchmark`.

`cargo-release` is configured (workspace `release.toml` and `metrics/release.toml`)
to do the `[Unreleased] -> [version] - date` substitution and tag/push at release
time. Tags follow the pattern `<crate-name>-v<version>`.

## Workflow at a glance

1. **Audit** — find unreleased changes per crate
2. **Plan** — version bumps, draft changelog entries, schedule PR notifications
3. **Confirm the plan with the user** before any destructive action
4. **Iterate per crate** in dependency order (one crate fully done — release +
   notifications + label removal — before starting the next; pause for explicit
   user confirmation between crates and before each `cargo-release --execute`)
5. **Sanity check** — `S-awaiting-release` queue should be empty

## Phase 1 — Audit

Two complementary signals; the git logs are authoritative because the label is
applied by hand and may be missing.

```bash
# Per-crate unreleased commits (authoritative)
git log --oneline <crate>-v<current_version>..HEAD -- <crate>/

# PRs explicitly flagged awaiting release (informative — may have gaps)
gh pr list --state merged --label "S-awaiting-release" --limit 100 \
  --json number,title,labels,mergedAt,url
```

Current versions live in each crate's `Cargo.toml`. There's also a helper:

```bash
./scripts/show-release-candidates.sh
```

For each commit, run `git show --stat <sha>` to see which crates' source files
it touched (a single PR may touch many crates — record every crate it touches,
because each gets its own changelog entry referencing the PR).

Skip these from the audit:
- Pure `chore(deps): bump X from A to B` dependabot commits — they ride along
  in the release without dedicated changelog entries unless they have user-
  facing impact (e.g., MSRV bump).
- `<!-- next-header -->`-only changes from prior cargo-release runs.

## Phase 2 — Plan

### Version bump assessment

For 0.x crates, semver treats minor as breaking:

- **Patch** (`0.x.y` → `0.x.(y+1)`): all changes are additive (new methods,
  new trait impls, `#[deprecated]` annotations, `const fn` qualifications,
  internal restructuring with private fields, performance changes, bug fixes).
- **Minor** (`0.x.y` → `0.(x+1).0`): any change that breaks downstream
  compilation — removed/renamed public items, changed signatures, changed
  trait bounds, MSRV bumps the public API depends on.

Default to patch unless evidence of breakage exists. Verify by looking at
public API diffs (`pub` items, trait signatures) — internal changes (struct
field types, hash algorithms) are not breaking even if behavior shifts.

### Inter-crate dependency cascade

Workspace deps are declared as `^0.24` (metrics) / `^0.20` (metrics-util) etc.
Caret on 0.x = `>=0.x.0, <0.(x+1).0`. So **patch bumps don't cascade** —
dependents pick up the new patch automatically. **Minor bumps do cascade** —
every dependent must update its dep declaration and re-release.

Dependency order for releases:

1. `metrics` — no workspace deps
2. `metrics-util` — depends on `metrics`
3. Exporters / tools (any order): `metrics-exporter-prometheus`,
   `metrics-exporter-dogstatsd`, `metrics-exporter-tcp`, `metrics-observer`,
   `metrics-tracing-context`
4. `metrics-benchmark` (publish=false) — optional housekeeping bump

### Changelog entries

Each released crate gets entries under `## [Unreleased] - ReleaseDate` in its
`CHANGELOG.md`. Use Keep-a-Changelog sub-headings: `### Added`, `### Changed`,
`### Fixed`, `### Deprecated`, `### Removed`. Each bullet ends with the PR link:
`([#NNN](https://github.com/metrics-rs/metrics/pull/NNN))`.

Keep the literal `## [Unreleased] - ReleaseDate` header in place; cargo-release
substitutes it at release time per the `pre-release-replacements` in the
`release.toml` files.

### When to skip a crate

- **`metrics-tracing-context`**: skip if the only changes are `Cargo.toml`
  metadata (toolchain/MSRV inheritance, etc.). It has no functional changes
  worth a release. Note: it tends to accumulate stale duplicated `Unreleased`
  entries (e.g., "Update `metrics-util` to `0.20`" repeated from a prior
  release) — clean those up the next time it gets a real release.
- **`metrics-benchmark`**: skip unless explicitly asked. `publish = false` and
  no `CHANGELOG.md` exists, so cargo-release fails on the
  `pre-release-replacements`. Manual `Cargo.toml` bump + tag is possible but
  rarely useful.
- **`metrics-observer`**: tends to also have stale duplicated `Unreleased`
  entries to clean up. Replace the stale block when adding new entries.

### PR comment scheduling

Each PR receives **exactly one consolidated comment**, posted during the
iteration that releases the **last** crate (in dependency order) that the PR
touched. Multi-crate PRs are deferred until that final crate ships, so the
comment can list every crate the work went out in. The `S-awaiting-release`
label (where present) is removed in the same step.

For each PR, build a "touched crates" list from `git show --stat <sha>`. The
last one in the release order above is the iteration where it gets notified.

#### Comment template

Single crate:

```
Released as `<crate>@v<ver>`.

Thanks for your contribution! :heart:!
```

Multi-crate (Oxford-comma):

```
Released as `<crate-1>@v<ver>`, `<crate-2>@v<ver>`, and `<crate-3>@v<ver>`.

Thanks for your contribution! :heart:!
```

#### Special cases

- **Maintainer's own PRs**: skip the comment ("Thanks for your contribution!"
  on your own PR is awkward), but still remove the label.
- **PRs without `S-awaiting-release`**: still post a comment if their work
  shipped (it's fine to thank a contributor whose label was missing); just no
  label to remove.
- **Pure metadata/toolchain PRs** (no user-facing crate impact, e.g., a
  rust-toolchain.toml removal that only edits Cargo.toml fields): comment is
  optional — the change has no specific crate to point at. Skip-by-default
  unless the user disagrees.

## Phase 3 — Confirm with the user

Write the plan into a plan file (or surface it inline if no plan-mode file is
provided), structured per this skill's example output:

- Per-crate: current → proposed version, list of PRs by category (Added /
  Changed / Fixed / Deprecated / Removed), draft changelog entries
- Release order
- PR comment schedule (which PR gets notified at which step)

Wait for explicit user approval before any commits or `cargo release --execute`.

## Phase 4 — Execute (per-crate iteration)

For each crate in dependency order:

### a. Edit the crate's `CHANGELOG.md`

Insert the drafted bullets under `## [Unreleased] - ReleaseDate`. Keep the
literal `Unreleased` heading. If the crate has stale duplicated `Unreleased`
content (typical for `metrics-observer` and `metrics-tracing-context`), clean
that up too.

### b. Commit the changelog edit

cargo-release **requires a clean working tree**. Commit changelog edits in a
dedicated commit before invoking `cargo release`:

```bash
git add <crate>/CHANGELOG.md
git commit -m "update CHANGELOG for <crate>"
```

**GPG signing**: this repo enforces signed commits. If the GPG agent isn't
unlocked, the commit fails with `gpg: signing failed: No such file or
directory` (pinentry can't reach a TTY from the agent context). Ask the user
to unlock GPG (`echo test | gpg --clearsign > /dev/null` from a TTY caches
the passphrase), then retry. **Do not bypass with `--no-gpg-sign`** unless
the user explicitly authorizes it.

### c. Dry-run cargo-release

```bash
cargo release -p <crate> patch
```

(No `--execute` flag = dry-run.) Inspect the output — confirm:
- Version bump is right (`Upgrading <crate> from X.Y.Z to X.Y.(Z+1)`)
- CHANGELOG substitution looks correct (your bullets land under `[X.Y.(Z+1)]`,
  fresh empty `[Unreleased]` added on top)
- Push target is `<crate>-v<new-version>`

### d. Pause for user confirmation before `--execute`

Crates.io publishes are **permanent** (you can yank but not delete). Even with
auto mode active, this is a destructive/external action and needs explicit
user authorization. Show the dry-run summary and wait for "go ahead".

### e. Execute the release

```bash
cargo release -p <crate> patch --execute --no-confirm
```

This: bumps `Cargo.toml`, stamps `CHANGELOG.md`/`RELEASES.md`, creates a
signed `chore: Release` commit + signed `<crate>-v<version>` tag, publishes to
crates.io, pushes `main` + tag to origin.

### f. PR notifications for this iteration

Run the comments + label removals for every PR whose final touched crate is
this one (per the schedule from Phase 2).

```bash
# Comment
gh pr comment <NUMBER> --body "$(cat <<'EOF'
Released as `<crate>@v<ver>`.

Thanks for your contribution! :heart:!
EOF
)"
```

```bash
# Label removal — DO NOT use `gh pr edit --remove-label`. It currently fails
# with a GraphQL error about Projects (classic) deprecation:
#   GraphQL: Projects (classic) is being deprecated...
# Use the REST API instead:
gh api -X DELETE /repos/metrics-rs/metrics/issues/<NUMBER>/labels/S-awaiting-release
```

Comments and label removals on different PRs are independent — run them in
parallel within a single message of tool calls.

### g. Pause before the next crate

Mark this crate's todo complete. Surface a summary (released version, PR
comments posted, labels removed). Wait for explicit user "go" before starting
the next crate.

## Phase 5 — Sanity check

After the last crate:

```bash
gh pr list --state merged --label "S-awaiting-release" --limit 100
```

Should be empty (modulo any PRs the user explicitly chose to skip — e.g.,
metadata-only PRs flagged optional in Phase 2).

Also useful:

```bash
git log --oneline -20            # see the release commits in order
git tag --list 'metrics*-v*' --sort=-creatordate | head -10
```

## Reference: example invocation

```bash
# Phase 1 audit
./scripts/show-release-candidates.sh
git log --oneline metrics-v0.24.3..HEAD -- metrics/
gh pr list --state merged --label "S-awaiting-release" --limit 100 --json number,title,labels,url

# Phase 4 per-crate (example: metrics)
git add metrics/CHANGELOG.md && git commit -m "update CHANGELOG for metrics"
cargo release -p metrics patch                              # dry-run, pause for user
cargo release -p metrics patch --execute --no-confirm       # publishes after user "go"
gh pr comment 654 --body "Released as \`metrics@v0.24.4\`. Thanks for your contribution! :heart:!"
# (PR #654 unlabeled in this case, otherwise:)
# gh api -X DELETE /repos/metrics-rs/metrics/issues/654/labels/S-awaiting-release
```

## Gotchas (from prior runs)

- **GPG agent must be unlocked** before commits. Ask the user to unlock from a
  TTY-capable terminal first; don't bypass with `--no-gpg-sign`.
- **`gh pr edit --remove-label` is broken** (Projects-classic deprecation
  error). Use `gh api -X DELETE /repos/metrics-rs/metrics/issues/<n>/labels/S-awaiting-release`.
- **cargo-release skips dirty workspace members by default** — the warnings
  about "skipping <crate> which has files changed since <crate>-v<X>" are
  expected when releasing one crate at a time; ignore them.
- **Don't try to release `metrics-benchmark` via cargo-release** unless you
  first create a `metrics-benchmark/CHANGELOG.md`. The
  `pre-release-replacements` rule in the workspace `release.toml` requires
  every crate to have one. The plan should mark it optional/skip.
- **Crates.io publish wait**: cargo-release prints
  "note: waiting for <crate> v<ver> to be available at registry `crates-io`."
  This may take 30s–2m. Don't treat the wait as a hang.
- **Push warning about vulnerabilities**: each push to main prints
  "GitHub found N vulnerabilities (... high, ... low)". Unrelated to the
  release; mention it as a follow-up but don't block on it.
- **Stale `Unreleased` changelog entries**: `metrics-observer` and
  `metrics-tracing-context` tend to have leftovers from prior cycles
  (e.g., "Update `metrics-util` to `0.20`" duplicated). Clean them up when
  editing those changelogs.
