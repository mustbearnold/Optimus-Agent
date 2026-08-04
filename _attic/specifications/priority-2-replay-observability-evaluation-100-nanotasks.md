---
doc_id: specifications-priority-2-replay-observability-evaluation-100-nanotasks
doc_type: history
plane: history
status: historical
authority: historical
summary: - Task range: n101–n200, exactly 100 tasks - Repository: mustbearnold/Optimus-Agent - Delivery: verified milestone commits pushed directly to origin/main; GitHub Issues are the work ledger; no branches or pull requests - Execution: one...
reviewed_on: 2026-07-31
review_by: never
knowledge_type: specification
owns:
  - docs/specifications/priority-2-replay-observability-evaluation-100-nanotasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/src/eval.rs
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
watches:
  - crates/optimus-packs/src/**
  - crates/optimus-kernel/src/**
  - docs/architecture/**
  - docs/maps/**
covers:
  - docs/specifications/priority-2-replay-observability-evaluation-100-nanotasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
  - crates/optimus-eval/src/evaluation.rs
  - crates/optimus-eval/src/eval.rs
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
depends_on:
  - docs/specifications/priority-1-integrity-100-nanotasks.md
  - docs/contracts/high-risk-contracts.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0022-versioned-agent-and-workflow-contracts.md
validated_by:
  - crates/optimus-packs/tests/packs_budget.rs
  - crates/optimus-eval/tests/evaluation_contracts.rs
  - scripts/test_engineering_memory.py
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
---

# Priority-2 replay, observability, and evaluation: 100 nano-tasks

- **Status:** Accepted for ordered execution
- **Task range:** n101–n200, exactly 100 tasks
- **Repository:** `mustbearnold/Optimus-Agent`
- **Delivery:** verified milestone commits pushed directly to `origin/main`; GitHub Issues are the work ledger; no branches or pull requests
- **Execution:** one writer, no subagents, RED–GREEN slices, generated Engineering Memory never edited manually
- **Starting identity:** `7a06d3fca68a9a7f5ebfa8ca2535a09f88f65507`

## Problem statement

Priority-1 leaves one high-risk contract partial: C-13 deterministic replay and provenance. Execution manifests retain hashes and honest replay classes, but do not retain bounded replay fixtures, verify fixture integrity, or execute an exact offline replay. Optimus also lacks one cross-subsystem trace identity, canonical operational metadata in `ToolDesc`, measured provider outcomes, version-bound evaluation datasets, metric thresholds, and baseline comparisons. These omissions prevent a reviewer from proving that an offline execution is reproducible, that a routing decision is evidence-bound, or that an evaluation result belongs to an exact contract/model/tool candidate.

## Solution

Implement a CPU-first, offline-verifiable replay and evaluation layer without claiming deterministic reproduction of live model or external effects:

1. Extend execution manifests with immutable version/policy/trace bindings and bounded content-addressed fixtures.
2. Add a fail-closed replay planner/executor that replays only deterministic or fixture-replayable stages and verifies every hash.
3. Propagate canonical trace/span identities through route, model, tool, agent, workflow, and terminal records.
4. Move retry, idempotency, timeout, cancellation, and observability declarations into canonical `ToolDesc` metadata.
5. Persist bounded provider telemetry and let routing consume only fresh, policy-approved evidence.
6. Add versioned evaluation datasets, exact metrics/thresholds, baseline comparison, and candidate binding.
7. Refresh ADRs, maps, generated Engineering Memory, canonical gates, deterministic tree identity, and GitHub evidence.

## Invariants

- Replay never performs a network call or durable side effect.
- Missing, duplicate, oversized, corrupt, or hash-mismatched fixtures fail closed before execution.
- Live remote model/external/destructive stages remain non-replayable unless an exact accepted fixture exists.
- Replay produces a new report linked to the source manifest; it never rewrites historical execution evidence.
- Trace identity is bounded, validated, and propagated; it grants no permission.
- Telemetry is evidence, not authorization; privacy, capability, and budget policy still dominate routing.
- Evaluation reports bind exact suite, dataset, contract, tool catalog, route policy, and candidate hashes.
- Baseline regressions fail only against explicit versioned thresholds; no hidden “quality” heuristic is allowed.
- SQLite stores remain independently authoritative; no cross-database transaction is claimed.
- GPU support remains out of scope; all work runs on CPU.

## User stories

1. As an auditor, I want every replay decision tied to an immutable source manifest and exact fixture hashes.
2. As an operator, I want corrupt or incomplete fixtures rejected before any replayed stage runs.
3. As a security owner, I want replay structurally incapable of network, process, filesystem-write, or approval effects.
4. As a developer, I want deterministic offline turns replayed and compared without provider access.
5. As an incident reviewer, I want one trace identity linking route, model, tool, agent, workflow, and terminal evidence.
6. As a tool author, I want retry/idempotency/timeout/cancellation metadata owned by the canonical descriptor.
7. As a router, I want fresh measured success/latency/cost evidence without allowing telemetry to bypass policy.
8. As an evaluator, I want typed datasets and exact metrics rather than substring-only ad hoc checks.
9. As a maintainer, I want candidate-versus-baseline regressions reported deterministically.
10. As an engineer, I want source, tests, ADRs, generated memory, GitHub issues, and remote commit identity to agree.

## Ordered 100 nano-task contract

## Final disposition

All 100 contiguous tasks, n101 through n200, were implemented in order. Source
implementation is frozen at `09fddbc1b60a6b37f9f80680988ea5036a9b8eec`.
Observed final gates passed: workspace formatting, all-target/all-feature strict
Clippy, all-feature workspace tests, strict all-feature rustdoc, 36/36 desktop
Playwright tests, and 12/12 Engineering Memory semantic tests. ADR-0023 and the
current architecture/contracts/maps describe the bounded implementation and its
remaining live-effect, distributed-tracing, token/billing, retrieval, and GPU
limits. This specification is retained as the historical execution contract;
GitHub Issue #3 carries final delivery reconciliation.

### Phase A — Replay authority and fixture store

101. **n101 Freeze C-13 baseline.** Characterize current manifest/replay schema and partial coverage. **Proof:** focused characterization test passes on current behavior.
102. **n102 Add replay schema version.** Define canonical replay bundle/report version constants. **Proof:** serialization version test.
103. **n103 Canonical fixture identity.** Add bounded parsed SHA-256 fixture identity. **Proof:** valid/invalid table.
104. **n104 Fixture kind vocabulary.** Define model response, tool outcome, and stage metadata kinds. **Proof:** roundtrip table.
105. **n105 Fixture metadata contract.** Bind source manifest, stage identity, media type, byte length, and content hash. **Proof:** validation tests.
106. **n106 Fixture size policy.** Define per-fixture and per-bundle hard byte/count ceilings. **Proof:** exact-boundary tests.
107. **n107 Replay bundle contract.** Add immutable source identity, trace, versions, fixtures, and expected terminal hash. **Proof:** JSON roundtrip.
108. **n108 Replay bundle validation.** Reject duplicates, missing stage fixtures, unsupported versions, and invalid terminal declarations. **Proof:** table tests.
109. **n109 Replay store schema.** Persist bundle metadata and fixture blobs in SQLite with foreign keys. **Proof:** create/reopen test.
110. **n110 Content-addressed insertion.** Store exact bytes once by hash; reject identity/content mismatch. **Proof:** duplicate/mismatch tests.
111. **n111 Atomic bundle insertion.** Insert metadata and fixtures in one transaction. **Proof:** late-failure rollback test.
112. **n112 Immutable source binding.** Reject a bundle for an absent or nonterminal source manifest. **Proof:** denial/no-row test.
113. **n113 Deterministic ordering.** List bundle fixtures in stage/kind/hash order. **Proof:** ordering test.
114. **n114 Bounded reads.** Read fixture bytes only after persisted length/hash validation. **Proof:** corruption test.
115. **n115 Corrupt metadata fencing.** Invalid enum/version/UUID rows fail closed. **Proof:** adversarial SQLite mutation.
116. **n116 Corrupt blob fencing.** Raw blob mutation fails before replay planning. **Proof:** hash-mismatch test.
117. **n117 Bundle reopen integrity.** Exact bundle survives independent connection/process reopen. **Proof:** reopen test.
118. **n118 Bundle deletion policy.** No public destructive delete; historical fixtures are immutable. **Proof:** public API review/compile test.
119. **n119 Export narrow APIs.** Expose validated contracts/store without raw connection access. **Proof:** rustdoc/API compile test.
120. **n120 Replay-store affected gate.** Run kernel tests, strict Clippy, and rustdoc. **Proof:** all pass.

### Phase B — Fail-closed replay planner and executor

121. **n121 Replay plan contract.** Define ordered stages with source hashes and required fixture IDs. **Proof:** roundtrip.
122. **n122 Classification lattice.** Define exact dominance: ambiguous > non-replayable > fixture > deterministic. **Proof:** exhaustive table.
123. **n123 Model-stage planning.** Offline/fixture model calls require exact response fixture. **Proof:** planner tests.
124. **n124 Tool-stage planning.** Deterministic/convergent tools use fixture-only comparison; external/destructive remain blocked. **Proof:** table tests.
125. **n125 Terminal-stage planning.** Bind expected terminal status and report hash. **Proof:** plan validation.
126. **n126 Missing-fixture denial.** Planner fails with typed blocker before execution. **Proof:** no-stage-run test.
127. **n127 Extra-fixture denial.** Unreferenced fixture fails closed to prevent evidence smuggling. **Proof:** focused test.
128. **n128 Version mismatch denial.** Contract/tool/policy/model version drift blocks replay. **Proof:** mutation table.
129. **n129 Trace mismatch denial.** Bundle/source trace disagreement blocks replay. **Proof:** focused test.
130. **n130 Replay sandbox contract.** Executor accepts no runtime, network, filesystem-write, approval, or provider handles. **Proof:** type/API characterization.
131. **n131 Replay scripted model.** Hydrate exact `CompletionResponse` fixtures without provider calls. **Proof:** offline model test.
132. **n132 Replay tool outcomes.** Hydrate and validate canonical `ToolOutcome` fixtures without invoking effectors. **Proof:** no-effect test.
133. **n133 Hash every consumed fixture.** Reverify bytes immediately before stage comparison. **Proof:** mutation-between-plan/run test.
134. **n134 Compare model request hashes.** Divergent prompt/history/tool schema fails at exact step. **Proof:** mismatch diagnostic test.
135. **n135 Compare tool argument hashes.** Divergent call identity/arguments fail at exact call. **Proof:** mismatch diagnostic test.
136. **n136 Compare outcome hashes.** Divergent terminal/tool envelope fails deterministically. **Proof:** mismatch test.
137. **n137 Replay report contract.** Record source/bundle identity, stage results, blockers, terminal comparison, and report hash. **Proof:** roundtrip.
138. **n138 Exactly one replay terminal result.** Success/failure/cancelled/ambiguous settlement is immutable. **Proof:** repeated-settlement test.
139. **n139 Persist replay reports.** Append report linked to source and bundle without rewriting either. **Proof:** reopen/order test.
140. **n140 Successful offline replay trajectory.** Replay a memory/tool/final-answer turn with zero effects. **Proof:** integration test.
141. **n141 Failed mismatch trajectory.** One changed fixture yields failed report and no later stages. **Proof:** integration test.
142. **n142 Ambiguous source trajectory.** Ambiguous source remains non-executable and reports exact blocker. **Proof:** integration test.
143. **n143 Remote source without fixture.** Live remote manifest remains non-replayable. **Proof:** denial test.
144. **n144 Remote source with accepted fixture.** Exact fixture permits offline comparison but never claims provider rerun. **Proof:** classification test.
145. **n145 Replay milestone gate.** Run kernel/packs/runtime affected suites and strict Clippy. **Proof:** pass.

### Phase C — Trace identity and canonical tool operations

146. **n146 Canonical trace identity.** Add bounded parsed trace/span IDs and parent relationship. **Proof:** valid/invalid tests.
147. **n147 Trace event contract.** Define ordered event kind, subsystem, subject, time, and evidence hash. **Proof:** roundtrip.
148. **n148 Trace store schema.** Persist immutable spans/events with one terminal span outcome. **Proof:** create/reopen test.
149. **n149 Trace parent validation.** Reject missing parent, cycle, duplicate span, and trace mismatch. **Proof:** adversarial tests.
150. **n150 Route trace binding.** Persist trace/span IDs with route decision. **Proof:** route ledger test.
151. **n151 Execution trace binding.** Manifest stores canonical trace/span rather than unvalidated text. **Proof:** migration/roundtrip.
152. **n152 Model-call span.** Record provider/model request/response evidence under child span. **Proof:** ordering test.
153. **n153 Tool-call span.** Record call/outcome/effect evidence under child span. **Proof:** integration test.
154. **n154 Agent trace binding.** Validate invocation request trace against linked effects. **Proof:** mismatch denial.
155. **n155 Workflow trace binding.** Node observability link validates exact trace/span. **Proof:** adapter trajectory.
156. **n156 Session terminal trace.** Accepted and terminal events reference same root trace. **Proof:** resume test.
157. **n157 Trace settlement fencing.** Late/duplicate terminal span result is rejected. **Proof:** stale completion test.
158. **n158 Tool retry metadata.** Add bounded retry declaration to `ToolDesc`. **Proof:** descriptor/catalog tests.
159. **n159 Tool idempotency metadata.** Add none/keyed/convergent declaration. **Proof:** consistency tests.
160. **n160 Tool timeout metadata.** Add finite timeout or explicit caller-bounded declaration. **Proof:** boundary tests.
161. **n161 Tool cancellation metadata.** Add cooperative/terminal/unsupported declaration. **Proof:** table tests.
162. **n162 Tool observability metadata.** Require call/span/provenance event declarations. **Proof:** catalog completeness.
163. **n163 Tool metadata consistency.** Reject impossible replay/idempotency/retry/policy combinations. **Proof:** invalid table.
164. **n164 Canonical catalog migration.** Populate exact metadata for all available and unavailable tools. **Proof:** catalog snapshot test.
165. **n165 Kernel dispatch consumes metadata.** Timeout/cancellation/replay decisions come from descriptor, not duplicate matches. **Proof:** drift regression.
166. **n166 Engineering Memory extracts metadata.** Remove hardcoded operational metadata. **Proof:** extractor RED/GREEN.
167. **n167 Trace/tool affected gate.** Run packs/kernel tests, Clippy, and rustdoc. **Proof:** pass.

### Phase D — Provider telemetry and evidence-bound routing

168. **n168 Telemetry observation contract.** Define provider/model, route, trace, outcome, latency, cost, and observed time. **Proof:** roundtrip.
169. **n169 Telemetry bounds.** Reject zero/overflow latency, unbounded cost, invalid IDs, and future-skewed time. **Proof:** table.
170. **n170 Telemetry store schema.** Append immutable observations with exact route/trace identity. **Proof:** create/reopen.
171. **n171 Telemetry provenance.** Reject observations for absent route or mismatched provider/model. **Proof:** denial/no-row.
172. **n172 Freshness policy.** Define explicit maximum age evaluated at caller-provided time. **Proof:** boundary test.
173. **n173 Aggregate health.** Compute bounded recent success/failure counts without floating nondeterminism. **Proof:** fixture table.
174. **n174 Aggregate latency.** Compute integer min/max/median/p95 over bounded samples. **Proof:** table.
175. **n175 Aggregate cost.** Compute checked integer totals/means. **Proof:** overflow tests.
176. **n176 Route request telemetry policy.** Add optional minimum success sample/rate and latency ceiling. **Proof:** serialization.
177. **n177 Policy-first routing.** Privacy/capability/budget rejection occurs before telemetry ranking. **Proof:** denial test.
178. **n178 Missing telemetry behavior.** Explicit allow/deny policy; no silent fallback. **Proof:** table.
179. **n179 Stale telemetry behavior.** Stale evidence cannot influence selection. **Proof:** fake-time test.
180. **n180 Deterministic candidate ranking.** Rank policy-approved candidates by declared integer tuple and stable ID tie-break. **Proof:** permutation test.
181. **n181 Persist evidence snapshot.** Route decision records telemetry policy and aggregate hashes. **Proof:** ledger test.
182. **n182 Record model outcomes.** Successful/failed/cancelled model attempts append telemetry once. **Proof:** integration test.
183. **n183 Retry does not double count.** Exact attempt identity fences duplicate observation. **Proof:** duplicate test.
184. **n184 Routing telemetry trajectory.** Fresh failure/latency evidence selects allowed fallback; policy denial still dominates. **Proof:** integration test.
185. **n185 Routing milestone gate.** Run kernel/CLI/desktop affected tests and strict Clippy. **Proof:** pass.

### Phase E — Versioned evaluation datasets, metrics, and baselines

186. **n186 Dataset and case contracts.** Add canonical dataset ID/version/hash and typed exact text/tool/terminal/replay/trace expectations. **Proof:** valid/invalid roundtrips.
187. **n187 Dataset validation and loader.** Reject duplicates, unknown tools, unsupported metrics, missing provenance, corrupt JSON, and oversized fixtures. **Proof:** table/fixture tests.
188. **n188 Metric vocabulary.** Define exact-match, tool precision/recall, terminal accuracy, replay accuracy, latency, and cost metrics. **Proof:** serialization.
189. **n189 Integer metrics and thresholds.** Use checked rational counts/basis points with explicit zero-denominator and sample policies. **Proof:** boundary table.
190. **n190 Candidate-bound report.** Bind dataset, contracts, tool catalog, route policy, provider/model, source tree, cases, metrics, thresholds, and report hash. **Proof:** roundtrip/mismatch tests.
191. **n191 Baseline store and comparison.** Persist immutable accepted reports and deterministically classify improved/equal/regressed metrics. **Proof:** reopen/table tests.
192. **n192 Integrity suite migration.** Represent six integrity cases in the typed dataset/report form. **Proof:** exact case-set regression.
193. **n193 Offline trajectory migration.** Represent four built-ins with exact expectations and replay evidence. **Proof:** all pass.
194. **n194 Deterministic evaluation integration.** Run candidate/baseline twice and prove byte-identical metrics/report hash. **Proof:** integration test.
195. **n195 Evaluation milestone gate.** Run kernel/packs affected tests and strict Clippy. **Proof:** pass.

### Phase F — Authority, generated memory, canonical gates, and delivery

196. **n196 ADR and authority docs.** Write ADR-0023 and update architecture/contracts/maps while retaining fixture-only replay limits. **Proof:** rich ADR lint and contradiction scan.
197. **n197 Disposition and Engineering Memory extractors.** Record n101–n200 evidence and emit replay/trace/tool/telemetry/evaluation claims only from source/tests. **Proof:** task cardinality plus extractor RED/GREEN.
198. **n198 Generate and strict-validate Engineering Memory.** Never edit generated JSON directly. **Proof:** VALID and CURRENT.
199. **n199 Canonical and exact-tree gates.** Run format, strict Clippy, full tests, strict rustdoc, desktop Playwright, and independent indexed-tree verification. **Proof:** all pass with exact count/hash.
200. **n200 Verified main delivery.** Freeze candidate, post evidence, commit/push once, read GitHub SHA/tree, close issue, and confirm clean `main`. **Proof:** local HEAD = origin/main = GitHub API SHA; issue closed; no PR/branch.

## Out of scope

- Local-model, embedding, vector, graph, reranking, CUDA, or GPU implementation.
- Paid/live-provider evaluation calls; telemetry tests use synthetic observations and local HTTP fixtures.
- Re-executing process, network, browser mutation, or filesystem-write effects during replay.
- Claiming exact replay of live nondeterministic providers without accepted fixtures.
- Distributed tracing infrastructure, message brokers, cross-database transactions, or remote agents.
- Feature branches, pull requests, rebases, history rewriting, release tags, or deployment.

## Completion rule

A nano-task completes only when its named proof is observed on the post-edit candidate. Milestone delivery is permitted only after affected gates pass. Final completion requires strict Engineering Memory currentness, full canonical gates, deterministic tree identity, exact remote read-back, issue closure, and a clean `main` worktree.
