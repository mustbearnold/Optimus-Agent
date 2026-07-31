---
doc_id: history-policy
doc_type: reference
plane: current
status: current
authority: canonical
summary: Optimus preserves decisions and delivery evidence because they explain how the system arrived here. Preservation does not make an old instruction current.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Documentation history policy

Optimus preserves decisions and delivery evidence because they explain how the
system arrived here. Preservation does not make an old instruction current.

## Precedence

1. Executable source and tests
2. `AGENTS.md` for repository development or `OPTIMUS_AGENTS.md` for product
   runtime behaviour
3. `docs/current/status.md` and `docs/current/roadmap.md`
4. Current contracts, maps, architecture and accepted ADRs
5. Current supporting specifications and how-to guides
6. Evidence for its named candidate and date
7. Historical plans, phase records, superseded specifications and old prose

## Rules

- Historical documents retain original claims and identifiers.
- Every historical or superseded document is classified as such in metadata.
- Historical content is excluded from default search/context unless explicitly
  requested.
- A current document may link to history for reasoning but may not delegate its
  present-tense authority to it.
- Contradictory current canonical documents are a gate failure; “agents should
  figure it out” is not an acceptable resolution.
- Evidence never generalizes beyond its named candidate, environment and date.
