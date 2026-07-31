---
doc_id: decisions-0059-standard-autonomy-is-consequence-bounded
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-31 - Program: program P30
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-policy/src/command_class.rs
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
depends_on:
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/plans/reliability-autonomy-program.md
validated_by:
  - crates/optimus-policy/tests/command_classification.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
---

# ADR-0059: Standard autonomy is consequence-bounded

- **Status:** Accepted
- **Date:** 2026-07-31
- **Program:** program P30

## Context

Standard project trust is the product's ordinary autonomy profile. It should
let Optimus read, edit, build, and test the selected project without turning
each exact effect into a permission card.

That promise was not yet a sound product default across every surface. Command
classification treated every unrecognised executable as project-local work, so
`git push`, network clients, and recognised command-string shell wrappers
entered the same auto-authorized lane as `cargo test`. Project deletes were
described as checkpointed even though Optimus does not yet create rollback
manifests. Confined child processes also inherit ambient credential variables,
but the runtime has no scoped credential-injection path that could replace
blanket inheritance without breaking authenticated builds.

The problem is not that Standard is too autonomous. The problem is that
authority was inferred from where a process started rather than from the
consequences Optimus could identify and contain.

## Decision

Standard remains the recommended ordinary-work profile, with three conservative
boundaries:

1. Direct, identifiable project build, test, formatting, and package operations
   keep their structured project capabilities and may be auto-authorized.
2. Commands whose argv identifies remote publication, direct network transfer,
   remote login, or a recognised command-string shell form such as `sh -c` do
   not enter the ordinary project-execution lane. They require a capability
   decision outside that lane.
3. Project deletes are irreversible until program P34 supplies real
   checkpoint/rollback manifests. A caller may not claim checkpointed
   reversibility merely because a path is project-scoped.

These rules are defense in depth. Classification is not semantic proof of what
an arbitrary binary will do, and the confined Linux envelope still permits
network access. A universal Standard default therefore also depends on a
network-authority boundary for arbitrary project processes and a scoped
credential-use path, including the owned-localhost lease tracked by R30.7.

## Reasons

- Consequence-shaped capability requests let routine work proceed while keeping
  an exact approval at identifiable remote and destructive boundaries.
- Classifying argv before the broker reuses Optimus's existing typed authority
  and receipt plane; it does not add a second UI-owned permission system.
- Marking deletion irreversible reports current recovery truth. A future
  checkpoint implementation can widen authority after it proves restoration.
- Recording residual arbitrary-process authority prevents the classifier from
  becoming a false security or release-default claim.

## Consequences

- Ordinary direct build and test commands remain low-friction.
- Known remote and opaque command forms pause before execution under Standard.
- Shell wrappers must be expressed as structured effects or receive a
  higher-authority decision; hiding a boundary action inside `sh -c` does not
  make it project-local.
- Delete-heavy workflows will ask until their rollback claim is real.
- React/Electron may continue offering Standard first. TUI, CLI, and host
  fallbacks must not be switched to Standard solely by changing a default enum;
  their outbound profile and approval-continuation behavior need regression
  coverage, and the remaining arbitrary-process network boundary must be
  closed first.

## Alternatives considered

**Make every surface default to Standard immediately.** Rejected for now. It
would remove visible approval friction by silently widening remote and
credential-bearing execution.

**Treat every command as opaque and require approval.** Rejected. That recreates
the permission wall for routine builds and tests even when Optimus has enough
structure to make a bounded decision.

**Search shell strings recursively for dangerous words.** Rejected as an
authority proof. Quoting, indirection, scripts, and alternate clients make
string inspection incomplete. Opaque wrappers leave the project lane instead.

**Strip ambient credentials from every confined command.** Deferred. Possession
of a token is not authority for an arbitrary build script to use it, but blanket
stripping also breaks approved pushes and private-registry builds. Optimus needs
a scoped credential-use path before inheritance can be removed without making
valid work impossible.

## Risks

- The command classifier is intentionally finite. Alternate clients,
  interpreters, project binaries, and scripts can still produce remote effects.
  This decision must not be cited as proof that arbitrary Standard commands
  have no network authority.
- Ambient credential inheritance remains a known boundary gap. The classifier
  reduces which recognised remote commands Standard auto-authorizes; it does
  not prevent arbitrary allowed processes from reading inherited variables.
- Reclassifying shell command strings makes some previously automatic project
  scripts ask. Structured argv or a directly invoked script restores the
  project-local path when its consequences fit that authority.

## Evaluation evidence

- `optimus-policy` unit coverage classifies direct project, package, remote, and
  opaque command forms.
- `command_classification` integration coverage proves Standard allows direct
  builds and asks for remote operations, opaque wrappers, and uncheckpointed
  deletes.
- `project_trust_profile` runtime coverage proves transparent project scripts
  proceed while a recognised opaque shell command parks for approval.

## Conditions for reconsideration

Revisit the conservative opaque-command rule when arbitrary child processes
receive code-enforced per-command network and credential capabilities. Revisit
irreversible deletion when program P34 produces, verifies, and retains
checkpoint manifests that can actually restore the deleted paths.

## Relevant code

- `crates/optimus-policy/src/command_class.rs` — deterministic argv classifier
- `crates/optimus-policy/src/lib.rs` — capability request and reversibility
- `crates/optimus-runtime/src/policy_bridge.rs` — request-to-broker bridge

## Relevant tests

- `crates/optimus-policy/tests/command_classification.rs` — Standard boundary
  decisions for direct, remote, opaque, and delete effects
- `crates/optimus-runtime/tests/project_trust_profile.rs` — transparent project
  execution continues while a recognised opaque shell form parks
