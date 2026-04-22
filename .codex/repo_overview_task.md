# Repo Overview Maintenance

Use this routine whenever repository rules say to refresh `.codex/REPO_OVERVIEW.md`.

## Goal
Keep `.codex/REPO_OVERVIEW.md` accurate enough that a new contributor can understand:
- what the repo does now,
- which parts are incomplete or intentionally stubbed,
- what is known to be broken, risky, or constrained.

## Routine
1. Read the current `.codex/REPO_OVERVIEW.md`.
2. Inspect the code, docs, fixtures, examples, and CI scripts touched by the current work.
3. Update the overview so it reflects the current repository state, not the planned state.
4. Keep the overview focused on durable facts:
   - main components and responsibilities,
   - important contracts and entry points,
   - meaningful examples and fixtures,
   - current TODO/WIP areas,
   - real broken, failing, or constrained areas.
5. Remove stale claims, dead links, and references to files or behavior that no longer exist.
6. Avoid speculative roadmap prose unless it explains an active extension point or known limitation.
7. Before finishing, sanity-check that any paths mentioned in the overview actually exist with the same casing.

## Output expectations
- Prefer concise updates over churn.
- Preserve the existing section structure unless there is a strong reason to improve it.
- If the repository behavior changed materially, make sure the changed behavior is visible in the overview.
