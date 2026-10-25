# 0011 — The landing page

**Status:** accepted, 2026-10-25.

## Context

M4 asks for a landing page in French and English. Vigie and Recto each have one in `site/`, static, published to GitHub Pages. Decision 0006 removed hosted CI because the repository is private and the author does not pay for Actions minutes.

## Decision

* **`site/`, static:** `index.html` (French) and `en/index.html`, one stylesheet, one small script, a self-hosted typeface (Figtree, SIL OFL). No build step, no framework, no third-party request, no analytics.
* **Screenshots from the demo backend** (`--mock`, `DemoLooks`), never from the author's machine: a public page shows no one's own apps.
* **Published by a workflow run by hand** (`.github/workflows/pages.yml`, `workflow_dispatch` only), once the repository is public: Pages and Actions cost nothing for a public repository. It is not CI and runs nothing but the upload; 0006 stands. Its actions are pinned by commit, with a read-only default token.
* **Until then, nothing is published.** The page is previewed locally.

## Consequences

* Publishing needs, in order: the repository public, Pages set to "GitHub Actions", one run of the workflow.
* The download buttons point to `releases/latest`, which works once a release is published.
