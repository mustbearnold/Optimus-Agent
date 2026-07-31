---
doc_id: evidence-tui-human-simulation-2026-07-31
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: - Base commit: 1b8fc94c5d6809a33d6817587830e12a74885be9 - Debug binary SHA-256 after the fix: c4f96b42db22f0f784806054dcb411be220247f73fecadf6bc0cfd238eb47fd8 - Surface: native optimus terminal UI in an isolated tmux session -...
reviewed_on: 2026-07-31
review_by: never
---

# TUI human-simulation evidence — 2026-07-31

## Candidate

- Base commit: `1b8fc94c5d6809a33d6817587830e12a74885be9`
- Debug binary SHA-256 after the fix:
  `c4f96b42db22f0f784806054dcb411be220247f73fecadf6bc0cfd238eb47fd8`
- Surface: native `optimus` terminal UI in an isolated tmux session
- Provider/model: `codex/gpt-5.6-sol`, thinking level `low`
- Autonomy profile: `review_changes`; command envelope: `confined`
- Evidence sources: visible TUI interaction plus read-only queries against
  `sessions.db` and `execution.db`
- Simulation markers were included in the first prompt of each persona. No
  credentials, raw database exports, or secret-bearing screenshots are stored
  in this report.

## Personas and conversations

### Elena — bakery owner

Durable session: `3c5a40ab-0f1e-4dee-bd56-f56e793448ab`

Elena asked for a prioritized 6am opening checklist, then asked Optimus to turn
it into a 5:15–6:00 timeline, save it as `opening-plan.md`, and report the exact
path. When the answer contained only the filename, she corrected it and asked
Optimus to verify the file and identify the largest operational risk.

**Confirmed current behaviour**

- The three-turn conversation stayed in one durable session and all three turns
  succeeded.
- The initial plan was practical and did not ask unnecessary planning
  questions.
- `list_dir` and `write_file` created the requested file in the Optimus
  workspace.
- The correction turn recovered: it returned the absolute path
  `/home/mustbearn/.local/share/optimus/workspace/opening-plan.md` and identified
  the milk delivery/new-helper overlap as the main opening risk.
- Durable evidence contains 13 messages. Turn durations were 32,433 ms,
  1,770 ms, and 4,863 ms.

**Notable failures**

- Saving one harmless workspace file required approval.
- The assistant ignored the explicit request for an exact path after the write.
- Correcting that omission routed read-only existence/path verification through
  `terminal`, causing a second approval. A built-in workspace operation should
  have completed the save-and-confirm flow without either interruption.

### Priya — New Zealand community researcher

Pre-fix failed session: `ede4ef1a-861a-4ff9-b9b9-992d969303ec`  
Pre-fix accidental follow-up session: `52b0c91c-4859-4a1d-942b-020cb7d90fdc`  
Post-fix retest session: `6c6927b7-ecdb-436e-9b67-67922dce5baa`

Priya asked for current New Zealand government help with home insulation and
efficient heating, requiring primary sources and explicit uncertainty. After a
failed source page, she naturally asked Optimus to continue using verified
facts and later requested a renter-versus-homeowner comparison.

**Confirmed pre-fix failure**

- Six `web_search` calls and `activate_pack` completed.
- `browser_navigate` returned a canonical outcome larger than 131,072 bytes.
  The turn failed with `pack_error` after 78,746 ms.
- The TUI reverted to `new session · ready`. Priya's follow-up silently created
  session `52b0c91c…`, and Optimus truthfully said it lacked the original
  question and gathered results. First-turn failure therefore broke
  conversational continuity despite the failed turn being durably recorded.

**Implemented repair**

- A new TUI turn now reserves its durable session identity on the worker before
  provider or tool work begins.
- The reserved identity is streamed to the screen before any terminal failure,
  without making submission block.
- Regression coverage forces a first-turn provider failure, submits a natural
  follow-up, and proves that exactly one durable session exists.

**Confirmed post-fix behaviour**

- The retest exposed session `6c6927b7…` while the first turn was still active.
- The same browser-size failure recurred, but the TUI stayed ready on
  `6c6927b7…`.
- Priya's natural follow-up remained in that session and produced a contextual,
  source-linked renter-versus-homeowner comparison.
- The failed research turn took 60,821 ms and recorded 19 completed tool calls:
  12 searches, six terminal calls, and one pack activation. It also recorded
  five approved actions and one denied action. The contextual recovery turn
  succeeded in 23,116 ms without another tool call.

**Remaining research and autonomy failures**

- The browser pack still rejects pages whose canonical result exceeds 128 KiB,
  terminating an otherwise recoverable turn.
- After browser/search degradation, one request proposed six near-duplicate
  terminal actions. Five were approved to gather bounded evidence; the sixth
  was denied to stop the loop.
- The first terminal fallback assumed the third-party Python `requests` package
  existed and failed with `ModuleNotFoundError`.
- Twelve search calls consumed 82,043 ms in aggregate; one call took 23,506 ms.
  The TUI showed activity, but not enough detail for a user to understand why
  ordinary research had become a minute-long approval sequence.
- Read-only public research should normally stay inside safe web/browser tools.
  Repeated shell fetch-and-parse commands under `review_changes` reproduce the
  permission-wall experience the product is intended to avoid.

### Marcus — household planner

Durable session: `5c82044c-1b93-4660-8756-957b3bc42bd3`

Marcus supplied four constraints: NZ$120, five dinners for two, one vegetarian,
and 30 minutes per night. He then asked to swap Tuesday and Thursday, preserve
the constraints, produce one grouped shopping list, and omit repeated cooking
instructions.

**Confirmed current behaviour**

- Both turns stayed in session `5c82044c…` and succeeded without tools or
  approvals.
- The follow-up retained the budget, time limit, household size, dietary
  requirement, and previous meal plan while applying the requested swap.
- It returned a grouped list without repeating the full recipes.
- The turns took 37,472 ms and 9,796 ms. The first answer was useful but
  disproportionately slow and long for a straightforward planning request.

## Outcome

The live scenarios separate two product qualities clearly:

1. Ordinary multi-turn conversational context works when the provider turn
   succeeds.
2. Before this change, failure before the first TUI turn completed orphaned the
   visible conversation. The repaired TUI now binds that failure and its
   follow-up to one durable session.

The greeting also now says `Esc clears a draft`, matching actual keyboard
behaviour; it no longer incorrectly claims that Escape exits.

## Follow-up priorities

1. Bound or summarize `browser_navigate` outcomes before the 128 KiB canonical
   result gate.
2. Make safe public research remain approval-free under the standard product
   autonomy profile, with terminal fallback treated as exceptional.
3. Teach file effects to report their resolved workspace path so save-and-confirm
   is one coherent operation.
4. Add repetition detection for materially equivalent tool proposals and
   surface a useful recovery choice before the user sees an approval loop.
5. Add durable TUI session browse, resume, search, and export surfaces so the
   high-quality underlying records are actually usable from the terminal UI.

## Deterministic verification

```text
cargo test -p optimus-tui --lib
  -> 181 passed

python3 scripts/tui_e2e.py --binary target/debug/optimus
  -> TUI_E2E_OK
```

The final managed gate result supersedes these focused counts.
