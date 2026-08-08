---
doc_id: spec-023-ecosystem-community
doc_type: reference
plane: work
status: current
authority: canonical
summary: Ecosystem mechanism for Optimus — a signed marketplace index with install/update/uninstall for skills and packs, a user trust store with a never-trust-by-default stance and a revocation list honored at install and update, third-party policy ceilings and schema-budget enforcement at install, outcome-gated candidate promotion for community skills, an install provenance ledger surfaced by doctor, and author-side pack signing tooling.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-packs/src/signed.rs
  - crates/optimus-skills/src/lib.rs
  - apps/optimus-cli/src/main.rs
  - crates/optimus-packs/src/catalog.rs
depends_on:
  - docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md
  - specs/006-memory-skills-packs/spec.md
---

# Spec-023: Ecosystem & community — signed marketplace mechanism

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED | B1: trust model unimplementable on HMAC-only TrustRoot (symmetric secret — no per-author fingerprints possible); 5 nits (skills-vs-packs install prose, bare marketplace refresh, command-root inconsistency, uncovered MUSTs, catalog-key pin mechanism) | B1: ed25519 per-author asymmetric scheme mandated (fingerprint = SHA-256 of public key; signed.rs gains asymmetric path; HMAC root reserved for operator-local signing); A2/A7 pin it; 5 nits applied incl. A8-A10 (pack_in_use, post-install revocation, ledger doctor) (round 2) |
| 2 | REJECTED | B1: 3 uncovered MUSTs (R2 dispatch refusal, R5 ledger durability, R6 key 0600); 4 nits (stale duplicate A7, trust key-material source, tampered-artifact case, hand-rolled-crypto risk) | B1: A11-A13 added; nits applied: duplicate A7 deleted, key-source + fingerprint-confirm flow, hash-mismatch case in A2, vetted crate (ed25519-dalek) mandated (round 3) |
| 3 | REJECTED | B1: round-2 fix itself left pack_fingerprint_mismatch uncovered; 3 nits (A1 stale-cache/URL/refresh coverage, HMAC-index fixture, unnamed A13 diagnostic) | A14 added; A1 extended (stale cache, config URL, refresh-only, HMAC-index refused); A13 names pack_key_permissions_too_open (round 4) |
| 4 | APPROVED | 2 non-blocking nits (fresh-config pin refusal case; uninstall success-path removal) | Applied 2026-08-08 (A1 fresh-config refusal; A8 full-removal assertion) |

## Purpose

Optimus's skills and packs are strong mechanically and zero
ecosystemically: skills are outcome-gated and permission-closed (a
defensible win), packs are signed and budgeted, but there is no way to
fetch a capability from anyone else — no community catalog, no
marketplace, and the CLI's `skills`/`packs` subcommands are
list/create/resolve only. Hermes, the parity target, has a plugin
system and a community skill catalog.

This spec defines the ecosystem MECHANISM without weakening the wins:
a signed marketplace index, install for skills and
install/update/rollback/uninstall for packs, a user trust store with
never-trust-by-default, a revocation list honored at install and
update, third-party policy ceilings and schema-budget enforcement at
install time, outcome-gated candidate promotion for community skills
(they earn trust, they don't inherit it), an install provenance ledger
surfaced by `optimus doctor`, and author-side signing tooling.
Building the actual public catalog is deliberately out of scope —
this is the pipe, not the water.

## Current state (Confirmed behaviour)

- Pack signing exists end-to-end: `crates/optimus-packs/src/signed.rs`
  provides `TrustRoot`, `PackManifestBody`, `SignedPackManifest`,
  `sign_manifest`, `verify_manifest`, `load_signed_manifest_file`,
  `assert_policy_within_ceiling`, and
  `default_third_party_ceiling()` (the max policy a third-party pack
  may declare) (Confirmed: source).
- The skills lifecycle is outcome-gated and permission-closed:
  `crates/optimus-skills/src/lib.rs` `SkillRegistry` creates
  candidates, records outcomes, promotes on evidence, pins,
  deprecates, and `authorize()` enforces declared permissions
  (Confirmed: source; the scorecard's "Outcome-gated,
  permission-closed Skills lifecycle" win).
- CLI surfaces today: `skills list/create/resolve` and
  `packs list` (+ budget demo); there is no install/update/uninstall
  or marketplace command (Confirmed: `apps/optimus-cli/src/main.rs`
  command enum).
- ADR-0068 requires every catalog row to dispatch or not exist; the
  schema budget (`PackBudgetConfig`) caps a session's loaded schema
  tokens (Confirmed: ADR + pack budget tests).
- The scorecard's wins include "Fail-closed tool ads↔handler registry
  + progressive pack schema budget" and "Outcome-gated,
  permission-closed Skills lifecycle" (Confirmed: scorecard).

## Requirements

### R1. Signed marketplace index

- The marketplace index MUST be a signed JSON document: entries for
  skills and packs (id, name, description, versions with artifact
  SHA-256 + signature over the version manifest, author, declared
  policy ceiling, declared permissions) (MUST).
- `optimus packs marketplace list` MUST fetch and verify the index
  (signature against a pinned catalog key — the key is
  config-stored as a fingerprint + public key; the pin is
  established by an explicit `packs trust` bootstrap command, never
  implicitly on first fetch), then list entries; an index with an
  invalid signature MUST be refused with
  `marketplace_index_signature_invalid` (MUST).
- Index caching MUST be content-addressed (hash-verified); a stale or
  tampered cache MUST be re-fetched, never served (MUST).
- The index URL MUST be config; refresh is manual
  (`packs marketplace refresh`) in v1 (MUST).

### R2. Install, update, uninstall

- `optimus packs install <id>@<version>` MUST download the artifact,
  verify its signature against the trust store (R3) AND the index's
  recorded hash, enforce `assert_policy_within_ceiling` and the
  schema budget, then install (MUST).
- `optimus skills install <id>@<version>` MUST do the same and land
  the skill as a CANDIDATE in the existing registry — community
  skills MUST NOT bypass the outcome-gated promotion path
  (MUST; the win is preserved).
- `optimus packs update <id>` MUST re-verify signature, revocation,
  and budget BEFORE replacing; the previous version MUST be retained
  until the new one verifies, and `packs rollback <id>` MUST restore
  it on failure (MUST).
- `optimus packs uninstall <id>` MUST remove every installed file and
  ledger row for that id, and MUST refuse when the pack has active
  sessions (named diagnostic `pack_in_use`) (MUST).
- A pack whose tools cannot dispatch (no handler) MUST be refused at
  install (MUST; ADR-0068).

### R3. Trust store and revocation

- The signing scheme MUST be per-author ASYMMETRIC: each author holds
  an ed25519 keypair; the fingerprint is the SHA-256 digest of the
  public key; `packs trust <fingerprint>` stores the public key;
  verification is by public key. The existing HMAC `TrustRoot`
  (signed.rs — symmetric secret) MUST stay reserved for
  operator-local builtin signing and MUST NOT be the marketplace
  scheme; `signed.rs` MUST gain the asymmetric verification path as
  an extension of the same canonical manifest format; the ed25519
  implementation MUST use a vetted crate (e.g. `ed25519-dalek`), not
  a hand-rolled implementation following the HMAC precedent (MUST).
- `packs trust <fingerprint>` MUST read the public key material from
  an explicit source (the pack artifact's manifest, the index entry,
  or a keyfile path passed to the command) and MUST store the key
  ONLY when its SHA-256 digest equals the given fingerprint; a
  mismatch MUST be refused with `pack_fingerprint_mismatch` (MUST).
- Trust MUST be user-confirmed: `packs trust <fingerprint>` adds a
  key; nothing is trusted by default (never-trust-by-default) (MUST).
- Install of a pack signed by an untrusted key MUST fail with
  `pack_key_untrusted`, naming the fingerprint and the command to
  trust it explicitly (MUST).
- A signed revocation list (blocked key fingerprints + pack ids) MUST
  be honored at install AND update; a revoked pack MUST be refused
  with `pack_revoked` (MUST).
- Revoking a key or id after install MUST NOT silently break running
  sessions, but MUST mark the pack `revoked` in the ledger and block
  further updates/execution of new sessions (MUST).

### R4. Policy and budget ceilings at install

- A pack declaring a policy above `default_third_party_ceiling()` MUST
  be refused at install with the named diagnostic
  `pack_policy_exceeds_ceiling` (MUST).
- A pack that would overflow the session schema budget MUST be refused
  with the existing `PackError::SchemaBudget` path (MUST).
- Declared permissions MUST be enforced at runtime by the existing
  `authorize()` mechanism; a community skill attempting an undeclared
  permission MUST be denied and recorded (MUST; permission-closed).

### R5. Provenance ledger and doctor

- Every install/update/uninstall/rollback MUST append a ledger row:
  id, version, catalog URL, artifact SHA-256, signing-key
  fingerprint, timestamp, outcome (MUST).
- `optimus doctor` MUST report installed marketplace items
  (id, version, trust status, revocation status, policy ceiling) and
  MUST NOT leak the ledger's hashes into prose diagnostics
  (MUST; named issues when a ledger row is inconsistent).
- The ledger MUST be durable (same SQLite authority pattern as the
  gateway/ops stores) (MUST).

### R6. Author-side signing tooling

- `optimus packs sign` MUST generate an ed25519 keypair (or use an
  existing key from the config home), sign a pack manifest, and emit
  the signed artifact + fingerprint for catalog submission (MUST).
- `optimus packs verify <artifact>` MUST verify a signed artifact
  locally (author-side pre-submission check) (MUST).
- Signing key material MUST obey the shared secret discipline: key
  files in the config home, mode 0600 (MUST; spec-018 R6).

## Acceptance criteria

- [ ] A1. Given a fixture signed index, when `optimus packs
  marketplace list` runs, then entries render; given a tampered index,
  then `marketplace_index_signature_invalid` is returned with nothing
  cached; given an HMAC-signed (non-ed25519) index, then it is
  refused; a stale cache is re-fetched, never served; the URL comes
  from config and `packs marketplace refresh` is the only fetch
  trigger; given a fresh config with NO pinned catalog key, then
  `marketplace list` refuses with the pin diagnostic and fetches
  nothing (R1, R3).
- [ ] A2. Given a signed fixture pack and a trusted key, when
  `optimus packs install` runs against an ed25519-signed fixture
  (fingerprint = SHA-256 of the public key), then the pack installs
  and lists; given an untrusted fingerprint OR a tampered artifact
  whose hash differs from the index record, then `pack_key_untrusted`
  / `pack_artifact_hash_mismatch` is returned with nothing written
  (R2, R3).
- [ ] A3. Given a fixture community skill, when `optimus skills
  install` runs, then it lands as a CANDIDATE (not promoted) and its
  declared permissions are enforced by `authorize()` (R2, R4).
- [ ] A4. Given a revoked pack id/key in the revocation list, when
  install or update is attempted, then `pack_revoked` is returned
  (R3).
- [ ] A5. Given a pack declaring policy above the third-party ceiling,
  when install is attempted, then `pack_policy_exceeds_ceiling` is
  returned; given a budget-overflowing pack, then the SchemaBudget
  error returns (R4).
- [ ] A6. Given a failed update, when `packs rollback` runs, then the
  previous verified version is restored and the ledger records both
  events; `optimus doctor` reports the item with trust/revocation
  status (R2, R5).
- [ ] A7. Given `optimus packs sign` + `packs verify` on a fixture
  manifest with a generated ed25519 keypair, when the cycle
  completes, then the artifact verifies against the public key and
  the fingerprint matches the key digest (R3, R6).
- [ ] A8. Given an installed pack with active sessions, when
  `optimus packs uninstall` runs, then it refuses with
  `pack_in_use` and nothing is removed; given no active sessions,
  when uninstall succeeds, then every installed file and ledger row
  for that id is gone and the pack no longer lists (R2).
- [ ] A9. Given a key/id revoked AFTER install, when a new session
  tries to execute the pack, then it is blocked; the ledger marks the
  pack `revoked` and running sessions are unaffected (R3).
- [ ] A10. Given a corrupted or inconsistent ledger row, when
  `optimus doctor` runs, then it reports the inconsistency as a named
  issue (exit 1) and no ledger hashes appear in the prose (R5).
- [ ] A11. Given a pack whose tools have no dispatchable handler, when
  install is attempted, then it is refused (ADR-0068) with nothing
  written (R2).
- [ ] A12. Given a crash mid-install, when the process restarts and
  `optimus doctor` runs, then the ledger row for the interrupted
  install is present and consistent (durable authority) (R5).
- [ ] A13. Given author key files written by `packs sign`, when their
  mode is checked, then they are 0600; when `optimus doctor` runs
  with a key at 0644, then it exits 1 with
  `pack_key_permissions_too_open` (R6; spec-018 A7 pattern).
- [ ] A14. Given `packs trust <fingerprint>` with a keyfile whose
  SHA-256 digest differs from the given fingerprint, when the trust
  command runs, then `pack_fingerprint_mismatch` is returned and no
  key is stored (R3).

## Out of scope

- Operating or hosting the public catalog (the mechanism is this
  spec; seeding a catalog is a follow-up).
- Monetization, ratings, moderation, or auto-refresh of the index.
- In-app marketplace browsing UI (the dashboard, spec-021, MAY render
  the marketplace later).
- Auto-update of installed packs (explicit `update` only, consistent
  with spec-018's update discipline).

## Open questions

- Whether the marketplace index should also carry an
  OS/architecture matrix for pack artifacts — default: yes, `target`
  field in the version record, refused on mismatch.
- Who hosts the pinned catalog key — the mechanism allows rotation
  via the revocation list; the initial pinned key is a bootstrap
  decision for the follow-up catalog effort.

## Links

- `crates/optimus-packs/src/signed.rs` — the signing/verification
  machinery R1–R6 build on.
- `crates/optimus-skills/src/lib.rs` — the outcome-gated,
  permission-closed registry community skills flow into.
- `apps/optimus-cli/src/main.rs` — the CLI surface this spec extends.
- `docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md`
  — dispatchability law (R2).
- `specs/006-memory-skills-packs/spec.md` — the owning packs/skills
  spec (amendment pattern).
- `specs/018-deployment-ops/spec.md` — shared secret + doctor
  discipline (R5, R6).
- `docs/architecture/sota-scorecard.md` — the wins this spec
  preserves (signed packs, skills lifecycle).
