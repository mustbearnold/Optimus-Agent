---
doc_id: decisions-0048-context-and-page-result-budgets
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0048: Context and page-result budgets are sized for tools, not for chat, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/compress.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-kernel/src/lib.rs
depends_on:
  - docs/decisions/0007-kernel-turn-loop.md
  - docs/decisions/0046-approval-resumes-the-turn.md
  - docs/decisions/0047-turn-step-budget.md
validated_by:
  - crates/optimus-kernel/tests/compression_turn.rs
---

# ADR-0048: Context and page-result budgets are sized for tools, not for chat

- **Status:** Accepted
- **Accepted:** 2026-08-01 — delivered: `CompressionConfig` carries 200k/24k with the convergence invariant and the run-time page split shipped in `page_extract`; pinned by `crates/optimus-kernel/tests/adr_budgets.rs`
- **Date:** 2026-07-27

## Context

`CompressionConfig` was sized for a conversation: a 48,000-char history, an
8-message verbatim tail, and everything between them replaced by a 120-char
extractive summary. That holds for chat. It does not hold for a turn that uses
tools, and the arithmetic says so before any run does.

One `browser_navigate` of a real page returned 23,000 chars — 48% of the whole
budget. The verbatim tail of a working turn held 36,000 of the 48,000 on its
own. Compression could therefore never get under the cap: it summarised the
middle on every step, stayed over budget, and summarised again. What it dropped
each time was real history; what caused the overflow sat in the tail and was
never touched.

Run against `github.com/trending`, that produced a turn which fetched the page
successfully, lost it to compression, searched for it again, fetched it again,
and exhausted its step budget without answering. Three defects were visible only
because the loop ran to the end:

1. The budget was too small to hold what the tools returned.
2. The tail was exempt from the budget, so compression could not converge.
3. The page result spent its own size on site furniture. `github.com/trending`
   renders its language filter as a menu naming every language GitHub knows —
   13,241 of the page's 16,136 extracted chars, ahead of the repository list.
   The model read it and reported that "only GitHub's surrounding navigation
   loaded".

## Decision

Three budgets, each stated where the thing being bounded is understood.

**The history budget is 200,000 chars** (`CompressionConfig::max_message_chars`),
up from 48,000 — roughly 50k tokens, comfortable inside any window Optimus
targets.

**No single tool result exceeds 24,000 chars in history**
(`CompressionConfig::max_tool_result_chars`). `keep_tail_messages *
max_tool_result_chars` must stay under `max_message_chars` — 8 × 24k = 192k <
200k — which is what makes compression converge rather than churn. The clamp
keeps the head and states the truncation.

**A page result is one budget split at run time**, not two constants
(`browser::MAX_RESULT_CHARS` = 22,000). Text is bounded first because it carries
what the page said; links get whatever the text did not use. Both state their
own truncation.

Two supporting rules follow from the same reasoning:

- **Site furniture is not page text.** `html_to_text` skips `CHROME_ELEMENTS`
  (`script`, `style`, `noscript`, `svg`, `nav`, `footer`, `select`, `template`,
  `details-menu`), and `extract_links` skips anchors inside them. `header` is
  deliberately absent — article headers carry the headline.
- **A preview must not answer a question about the data.** `summarize` bounds in
  chars, not bytes, and says that the *preview* stopped.

## Alternatives considered

**Raise `max_message_chars` alone.** Cheapest, and it does unblock the observed
turn. Rejected as sufficient: it leaves the tail exempt, so the same churn
returns at whatever size the tail reaches, and it pays full price for furniture.

**Summarise oversized results with an auxiliary model**, as Hermes does above 5k
chars. Strictly better than a head-cut, since truncation loses the tail and
`github.com/trending` puts its content there. Rejected *for now* because
`compress.rs` is explicitly extractive with no auxiliary LLM; changing that is
its own decision with its own latency and cost, not a side effect of a budget
fix. Recorded as the first thing to reconsider.

**Extract from `<main>` only**, the usual readability heuristic. Rejected on the
evidence: on the page that motivated this, the language menu is *inside*
`<main>`, so it removes the site header and keeps the actual problem.

**Fixed caps for text and links.** What was tried first, and it starved the page
that had room. The two vary inversely — stripping furniture cut the text from
16,136 to 3,855 chars, and the same page needs many links, because each
repository row spends one on the repository and three more on its stargazers,
forks and contributors. A cap sized for the worst case dropped the owner of
every repository but the first.

## Reasons

- A budget that a single tool result can half is not a budget.
- Whatever compression cannot touch must still be bounded, or it cannot
  converge. This is the whole of defect 2.
- The module that knows what a page *is* should decide what a page keeps. Left
  to the kernel's blind head-cut, `serde_json`'s key ordering decides instead:
  `links` sorts before `text`, so the field carrying the content is always the
  one cut.
- Truncation that does not announce itself is indistinguishable from a document
  that ended, and the difference decides whether to answer or fetch again.

## Consequences

- A turn can hold several page fetches and the conversation at once.
- Compression converges: the tail has a bound, so summarising the middle reaches
  the cap instead of churning.
- Page results shrank ~4× on furniture-heavy sites at no cost to content.
- More links survive, which is what makes `owner / name` unambiguous. Extraction
  flattens the pair onto separate lines; the href does not.

## Risks

- **200,000 chars is a guess against provider windows, not a measurement.** It is
  comfortable for the models Optimus targets today and would not be for a small
  local one. The three budgets are independent, so this can move alone.
- **`CHROME_ELEMENTS` is a blocklist**, and a site that renders content inside
  `<nav>` loses it. Mitigated by keeping the list to elements that are furniture
  by definition, and by leaving `header` out.
- **Head-truncation still loses the tail** when a result exceeds its budget. On
  pages that put content last — which is what started this — that is the wrong
  end to keep. Stripping furniture makes it rare rather than impossible.

## Evaluation evidence

Eight runs of `look up the github trending daily and summarise the top repos`
through the TUI against the live site, inspecting `messages_json` after each.

- Before: compression fired at 48,821 chars against a 48,000 cap; the successful
  fetch was dropped; the turn ended at the step budget with no answer.
- After the history and per-result budgets: 80,767 chars against 200,000, no
  compression, the user's request intact and verbatim at index 1.
- After furniture stripping: extracted text 16,136 → 3,855 chars; the first
  repository moved from 82% of the way through the text to 29%.
- After the run-time split: 162 links delivered, nothing truncated, 22 distinct
  repository URLs.
- Final run returned four repositories whose owners, names, URLs and star counts
  all match the page — `permissionlesstech/bitchat` 30.8k, `citrolabs/ego-lite`
  4.9k, `block/buzz` 13.7k, `pingdotgg/t3code` 15.1k.

The intermediate runs are the argument for the last two rules. With the budgets
raised but furniture still included, the model answered with one repository and
called the page "partially truncated by the retrieval tool" — it had read the
summary's ellipsis as a statement about the data. With furniture stripped but
links still capped from the head, it answered with `jackdorsey/bitchat` and
`egoist/ego-lite`; those repositories belong to `permissionlesstech` and
`citrolabs`, and their hrefs were on the page but past the cap.

## Conditions for reconsideration

- A provider whose window makes 200,000 chars unsafe.
- Auxiliary-model summarisation becoming acceptable in `compress.rs`, which
  would replace head-truncation with something that does not lose the tail.
- A page whose content is genuinely inside a `CHROME_ELEMENTS` element, which
  would mean the list needs to become structural rather than by tag name.
- Extraction gaining structure — Markdown rather than flattened text — which
  would make the link table less load-bearing for `owner / name`.

## Relevant code

- `crates/optimus-kernel/src/compress.rs` — `CompressionConfig`,
  `clamp_tool_results`
- `crates/optimus-kernel/src/browser.rs` — `MAX_RESULT_CHARS`,
  `page_to_tool_json`, `CHROME_ELEMENTS`, `chrome_spans`, `html_to_text`,
  `extract_links`
- `crates/optimus-kernel/src/lib.rs` — `summarize`

## Relevant tests

- `compress::tests::one_page_sized_result_cannot_eat_the_whole_budget`
- `compress::tests::the_verbatim_tail_alone_cannot_exceed_the_budget`
- `compress::tests::clamping_settles_instead_of_shaving_the_result_every_step`
- `browser::tests::a_filter_menu_does_not_bury_the_list_it_filters`
- `browser::tests::the_link_cap_is_not_spent_on_the_navigation_menu`
- `browser::tests::a_link_table_never_outspends_the_page_it_came_from`
- `browser::tests::ordinary_markup_is_still_read_as_text`
- `summarize_tests::a_multibyte_character_on_the_boundary_does_not_take_the_turn_down`
