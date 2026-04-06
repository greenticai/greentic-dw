# PR-04 — Add CLI wizard with i18n, AnswerDocument support, and greentic-dev integration contract

## Objective / Outcome
Provide a DW CLI and wizard experience aligned with the rest of Greentic, including i18n, `--answers`, `--schema`, `--emit-answers`, `--dry-run`, and a stable integration contract so `greentic-dev` can delegate into it cleanly.

## Repo status
New repo

## Depends on
- PR-01 bootstrap landed
- PR-02 types/manifest landed

## Scope
- Implement `greentic-dw-cli`.
- Add wizard UX compatible with AnswerDocument conventions.
- Support `--answers`, `--schema`, `--emit-answers`, `--dry-run`.
- Keep wizard strings localizable.
- Document command contract for `greentic-dev` delegation.

## Acceptance criteria
- DW CLI compiles and provides help output.
- Wizard supports AnswerDocument replay/capture/dry-run patterns.
- CLI text is structured for i18n in line with Greentic conventions.

## Codex prompt
```text
Implement the DW CLI and wizard.

Provide a Greentic-aligned CLI that supports:
- localized wizard UX
- `--answers`
- `--schema`
- `--emit-answers`
- `--dry-run`

Make the command structure straightforward for `greentic-dev` to delegate to this wizard.
Follow existing Greentic AnswerDocument conventions.
```
