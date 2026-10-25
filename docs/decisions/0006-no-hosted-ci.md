# 0006 — No hosted CI: checks run locally

**Status:** accepted, 2026-10-24

## Context

M0 added a GitHub Actions workflow on `windows-latest`. The repository is private, so Windows runner minutes are billed, and the author does not want to pay for them. The first runs were refused for billing. The workflow also checked little that a local run does not: CI runners have no audio device, so the owned-session test skips there, and shared runners are too noisy for the budget.

## Decision

* **No GitHub Actions workflow** in this repository. `.github/workflows/` stays empty.
* **`cargo xtask check`** runs what CI ran (`cargo check`, `cargo test`, `cargo clippy -- -D warnings`, all with `--locked`). It must pass before every push and is part of every milestone's exit criteria, in place of "CI green".
* **`cargo xtask bench-overhead`** is run by hand on an idle machine, as before.
* **Releases (decision 0005)** are built locally with `cargo xtask dist` (M4): installer, portable zip and `SHA256SUMS`, attached by the author to a draft GitHub release. This supersedes 0005's release workflow and its rules for workflows (read-only token, actions pinned by commit) until a workflow comes back. `--locked` release builds still apply.

## Consequences

* Nothing checks a push automatically: running `cargo xtask check` first is the author's and the agents' discipline.
* Coming back to a hosted CI (a public repository gets free minutes) takes a new decision record. M0's workflow is in the history (commit `b9c2e44`).
