# PR-05 — Add memory extension surface, testing harnesses, and nightly readiness

## Objective / Outcome
Define the DW memory extension surface without hardcoding a backend, and round out the repo with strong testing harnesses and scripts so nightly coverage, performance, and e2e jobs have concrete commands to call.

## Repo status
New repo

## Depends on
- PR-03 runtime landed
- PR-04 CLI landed

## Scope
- Add memory policy/provider reference contracts and extension points.
- Do not hardcode a concrete memory backend into core.
- Add `greentic-dw-testing` fixtures and conformance tests.
- Expose practical commands/scripts for coverage, perf, and e2e nightly jobs.
- Document how downstream memory packs plug in.

## Acceptance criteria
- Memory is represented as an extension surface, not a baked-in engine detail.
- Testing crate includes useful conformance fixtures.
- Coverage/perf/e2e commands are callable by nightly workflows.

## Codex prompt
```text
Implement the next maturity layer for `greentic-dw`.

Add:
- memory extension surface
- strong testing fixtures
- concrete nightly-invocable coverage/perf/e2e command hooks

Keep memory backend-agnostic.
Ensure the repo exposes commands or scripts that org nightly workflows can call.
```
