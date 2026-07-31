# TUI human-simulation evidence — 2026-07-31, round 2

## Candidate

- Base commit: `6f182691029fb47d7f77d13fa93e3bf2e6c06f56`
- Pre-fix debug binary SHA-256:
  `c4f96b42db22f0f784806054dcb411be220247f73fecadf6bc0cfd238eb47fd8`
- Post-fix debug binary SHA-256:
  `9484449dc6f5dd1bbd89acba07746870ee04cd17f504998aec16834e330d11d6`
- Surface: native `optimus` terminal UI in isolated tmux sessions
- Provider/model: `codex/gpt-5.6-sol`, thinking level `low`
- Autonomy profile: `review_changes`; command envelope: `confined`
- Evidence: visible TUI interaction, exact candidate binaries, and read-only
  queries against `sessions.db` and `execution.db`
- No credentials, raw database exports, or secret-bearing screenshots are
  stored here.

## New humans and conversations

### Wiremu — Wellington home gardener

Durable session: `4c3ab539-b6a9-4cf6-ad94-6018d9e352cb`

Wiremu asked Optimus to find current Wellington residential garden-watering
rules from primary sources, separate confirmed rules from weather-dependent
advice, and produce a practical seven-day plan. After the source page failed,
he asked Optimus to continue from the verified search evidence without using
the terminal.

**Confirmed current behaviour**

- Six `web_search` calls succeeded and consumed 47,193 ms in aggregate.
- `activate_pack` succeeded.
- `browser_navigate` again exceeded the 131,072-byte canonical outcome limit.
  The first turn failed with `pack_error` after 70,083 ms.
- The repaired TUI retained session `4c3ab539…`. The natural follow-up used the
  first turn's context and produced a source-linked rule summary and conditional
  watering plan without terminal actions.
- The recovery turn succeeded in 31,412 ms.

**Notable failure**

The browser-size defect is general, not specific to the earlier insulation
scenario. A legitimate council-research request again spent more than a minute
collecting evidence and then lost the turn at the page-navigation boundary.
The durable session repair prevents conversational loss, but it does not make
the original action complete.

### Claire — community-theatre treasurer

Pre-fix session: `cc708c15-ffff-4833-90ce-631633e5c708`  
Post-fix native retest: `9810db55-0feb-4b69-af6b-c35096aebaf7`

Claire supplied ticket revenue and four expenses, asked for the surplus and a
CSV in the workspace, and explicitly requested the full saved path without
unnecessary approvals.

**Confirmed pre-fix failure**

- Before calculating or writing, Optimus proposed `terminal` merely to run
  `pwd`. That approval was denied.
- It recovered to `write_file`; the actual write was approved and completed in
  26 ms.
- Because the successful tool result exposed only a workspace-relative path,
  Optimus tried two directory listings, then proposed Python
  `os.getcwd()` under another approval. That was denied.
- It then used `find_files` and attempted to read `/proc/self/mountinfo`.
  Confinement correctly denied the system path, but the turn failed with
  `fs_sandbox_deny` after 12,154 ms.
- The session recorded six completed tool calls, one approved action, and two
  denied actions. A natural follow-up recovered in 3,835 ms and honestly
  reported the relative path while saying the absolute path was unavailable.
- The original file exists at
  `/home/mustbearn/.local/share/optimus/workspace/show-finances.csv`.

**Implemented repair**

- Every successful `write_file` model-facing outcome now includes both
  `relative_path` and `absolute_path`.
- This applies to direct writes and writes that resume after an approval.
- Durable runtime receipts remain portable and content-addressed; the absolute
  host path is added only to the model-facing result.
- The canonical tool description now tells the model that `write_file` returns
  both paths and explicitly prefers it over terminal path discovery.

**Confirmed post-fix native behaviour**

- The unchanged work shape went directly to `write_file`; it did not run or
  propose `pwd`, Python, directory scans, `find_files`, or `/proc` reads.
- The write was the only tool call and the only approval.
- The turn succeeded in 3,901 ms.
- Optimus calculated the correct NZD 474.40 surplus and reported the exact path:
  `/home/mustbearn/.local/share/optimus/workspace/show-finances-green.csv`.
- The saved CSV exists at that path and is 171 bytes.

### Niko — overwhelmed statistics student

Durable session: `260d7851-c361-4dc9-b359-b844d1f0d3bb`

Niko had three chapters and two practice papers before a Friday exam, a
Wednesday evening shift, ADHD, and a 90-minute daily limit. The follow-up
cancelled the shift, marked Chapter 1 complete, and asked Optimus to rebuild
only Wednesday and Thursday around practice papers.

**Confirmed current behaviour**

- Both turns stayed in one session and succeeded without tools or approvals.
- The first answer retained all constraints and used three 25-minute blocks
  plus two five-minute breaks, keeping each day to 85 minutes.
- The follow-up correctly removed the cancelled shift, removed completed
  Chapter 1, preserved the daily cap and block length, and prioritized both
  practice papers.
- Turn durations were 24,771 ms and 14,945 ms.

## Outcome

Round 2 confirms three distinct product truths:

1. Successful ordinary conversations retain and update constraints well.
2. The first-round durable-session repair reliably preserves context after a
   failed research or file-tool turn.
3. A model-facing tool result must contain the information needed to finish the
   user's request. Returning only `show-finances.csv` caused a five-tool,
   two-denial path-discovery detour; returning `absolute_path` reduced the same
   work shape to one tool, one unavoidable write decision under the active
   profile, and a complete answer.

## Remaining priorities

1. Bound or summarize browser page output before canonical validation so pages
   larger than 128 KiB do not abort a researched turn.
2. Move normal workspace writes onto the consequence-bounded standard autonomy
   path so harmless file creation does not require routine confirmation.
3. Show more meaningful research progress than a repeated `web_search` label,
   especially when several searches take tens of seconds.
4. Continue fresh-human testing across local organization, scheduling,
   document work, recovery, and long-session navigation.

## Focused verification

```text
cargo test -p optimus-packs --test packs_budget \
  canonical_descriptor_owns_output_schema_and_replay_class
  -> 1 passed

cargo test -p optimus-kernel --test kernel_turn \
  project_write_emits_exact_approval_lifecycle_before_any_effect
  -> 1 passed

cargo test -p optimus-kernel --test kernel_turn \
  write_file_tool_uses_durable_job
  -> 1 passed
```

The final managed gate result supersedes these focused checks.
