---
doc_id: decisions-0002-memory-invariants
doc_type: decision
plane: decision
status: current
authority: record
summary: Accepted — 2026-07-18 (invariants locked; full store is Phase 2)
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0002: Memory invariants (MetaMemory-native)

## Status

Accepted — 2026-07-18 (invariants locked; full store is Phase 2)

## Context

Hermes uses tiny cache-stable pins plus optional deep providers. Deep memory is easy to get wrong (vector-as-truth, last-write-wins, authority laundering). Optimus must ship evidence-native memory that fits local GPU headroom constraints when embeddings are used.

## Decision

### Layers (compose, do not collapse)

1. **Core pin** — tiny curated USER/AGENT facts; cache-stable for a session segment.
2. **Working memory** — goal stack, constraints, open errors (session/job scoped).
3. **Immutable experience ledger** — append-only events (canonical).
4. **Episodic** — trajectories with attempts/outcomes.
5. **Semantic bitemporal claims** — valid time + transaction time; conflicts preserved.
6. **Procedural** — versioned skills (see skills lifecycle; not free-form memory rows).
7. **Artifacts** — content-addressed files/manifests.
8. **Meta-memory** — retrieval/outcome feedback.

### Security invariants (non-negotiable)

1. **Origin-bound authority** — storage never elevates trust.
2. **Evidence is not instruction** — recalls are fenced data with origin/trust/allowed-use.
3. **No durable action capability** in memory rows — actions need live capability tokens.
4. **Scope before top-k** — tenant/user/agent/project filters before candidate caps.
5. **Conflict preservation** — no silent last-write-wins.
6. **Privacy erasure** covers all projections.
7. **No destructive summarization** as sole surviving evidence.
8. **Write gate** before canonicalization (classify/redact/dedupe/quarantine).

### Phase mapping

- Phase 0: event ledger for Work Graph only (jobs/nodes/tools) — seeds the ledger design.
- Phase 2: full MetaMemory claim/recall EvidencePacket API + adversarial probes.
- Embeddings optional; default path must not require > headroom on RTX 5070 12GB class GPUs when agent work is concurrent.

## Consequences

- Positive: correct-by-construction memory story vs bolt-on RAG.
- Negative: Phase 2 is large; must not block Phase 0 resume.
- Pins stay small on purpose (Hermes cost lesson retained).

## Alternatives rejected

- **Pure vector DB as canonical memory**
- **Summaries replace ledger**
- **Remembered preference authorizes shell/network**
