---
doc_id: decisions-0074-a-surface-owns-the-catalog-it-answers-from
doc_type: decision
plane: decision
status: current
authority: record
summary: The terminal keeps one command catalog of its own that dispatch, the right-click menu, /help and predictive typing all derive from, and stops claiming to source names from the optimus-ops surface registry — a claim the code never honoured, and which the measured three-row overlap between the two lists shows it never could.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - apps/optimus-tui/src/commands.rs
  - apps/optimus-tui/src/completion.rs
  - apps/optimus-tui/src/keys.rs
  - apps/optimus-tui/src/view.rs
depends_on:
  - docs/decisions/0068-a-catalog-row-must-dispatch-or-not-exist.md
  - docs/decisions/0045-agent-host-and-surface-transports.md
validated_by:
  - apps/optimus-tui/tests/pty.rs
  - apps/optimus-tui/src/commands.rs
  - apps/optimus-tui/src/completion.rs
  - apps/optimus-tui/src/view.rs
---

# ADR-0074: A surface owns the catalog it answers from

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

The terminal's slash commands were three lists that nobody kept in step: a `match`
arm per name in `dispatch`, seven hard-coded tuples for the right-click menu, and a
seventeen-line array of aligned strings in `help()`. Adding a command meant editing
three places and no test noticed if you edited two.

The module's own docstring said otherwise. Landed at
`c61348c08df7028df0837a09fbb285beb9e002e3`, it read:

> Names come from the shared registry in `optimus-ops` (`builtin_surface_commands`),
> so the terminal cannot invent a command the other surfaces do not have — the
> registry stays the single catalog.

That was false in the ordinary sense: `apps/optimus-tui/Cargo.toml` has no
`optimus-ops` dependency and the file never called the function. It was not merely
aspirational, either — `builtin_surface_commands` is re-exported from
`crates/optimus-kernel/src/lib.rs:122` and the terminal already depends on
`optimus-kernel`, so the claim was one line of code away from being checkable, and
would have failed the check.

Measuring the two lists shows why nobody ever wrote that line:

| List | Rows |
| --- | --- |
| `optimus_ops::builtin_surface_commands()` | 16 |
| terminal `COMMANDS` | 12 names, 13 spellings with the `exit` alias |
| terminal right-click menu (before this change) | 7 |
| present in both the registry and the terminal | **3** (`help`, `new`, `yolo`) |

The registry catalogues what an *agent surface* offers a user across CLI and
Desktop — `sessions`, `packs`, `cron`, `artifacts`, `mail`. The terminal's list is
mostly things only a terminal has: `/mouse`, `/frame`, `/thinking`, `/model`,
`/access`. Thirteen of sixteen registry rows have no terminal implementation and
nine of twelve terminal commands are meaningless anywhere else. Sourcing one from
the other would have meant a filter that discarded 81% of its input.

Predictive typing needed a catalog to predict from. Writing a fourth hard-coded
list was not an option, and honouring the docstring was not one either.

## Decision

1. `apps/optimus-tui/src/commands.rs` holds exactly one catalog, `COMMANDS: &[Command]`.
   `dispatch`, the right-click menu, `/help` and predictive typing all derive from it.
   None of them carries a name the others do not.
2. That catalog is the terminal's own, and is deliberately not
   `optimus_ops::builtin_surface_commands()`. The two lists describe different
   things and are not required to agree.
3. Every catalogued row must dispatch — ADR-0068's rule applied to this surface. A
   row that a click cannot usefully reach because it needs an argument is *marked*
   (`menu: false`), not omitted, so the same catalog still answers `/help` and
   predictive typing for it.
4. The docstring's registry claim is deleted rather than made true. A comment that
   describes an import the file does not have is worse than no comment: it is the
   only place a reader would look to find out, and it sends them away satisfied.
5. Predictive typing is not a `Picker`. `keys::intent` opens with
   `if mode.picker { return picker_intent(key); }` — a picker owns the keyboard
   outright, which is right for a list the user asked for and wrong for one that
   appeared because they typed a slash. Suggestions borrow only Tab, Up and Down,
   only while the draft is a lone `/word`, and hand all three back otherwise.
6. The suggestion list is an overlay anchored to the composer's top edge, not a
   fourth row in the layout. `composer_height` is read by `view::draw`,
   `lib.rs::on_mouse`, `lib.rs::scroll_span` and `lib.rs::scroll_page`; a list that
   took height from the frame would move the prompt out from under the cursor the
   moment a `/` was typed and move it back on the next keystroke.

## Alternatives considered

**Import `optimus-ops` and filter the registry down to what the terminal
implements.** This is what the docstring described. It yields three rows. The other
nine terminal commands would still need a second list, so the change buys a
dependency and keeps the duplication it was meant to remove.

**Make the registry a superset and have the terminal refuse the rows it lacks.**
Directly contradicts ADR-0068: a catalogued row that refuses teaches a false
affordance. Predictive typing makes this materially worse than it was for tools —
the refusing names would be offered to the user's fingers as they type.

**Generate the terminal catalog from the registry at build time.** Same three-row
yield, plus a build step, plus a generated file under law 13. The problem is not
the mechanism; it is that the two lists are about different things.

**Keep the three lists and add a fourth for suggestions.** Cheapest edit. It makes
the failure mode — a command that dispatches but is unlistable, or listed but
undispatchable — one list more likely each time, and predictive typing is the
surface where that failure is most visible.

**Two-way sync: teach the registry the terminal's commands.** `/mouse` and `/frame`
are not agent-surface capabilities, and `commands_for_surface` has no honest
`CommandSurface` value for "terminal only". Widening a shared registry to hold one
surface's private vocabulary is how single catalogs stop being single.

## Reasons

- **The overlap is 3 of 16.** Two lists that share three rows are two lists. Naming
  one the source of the other is a fiction that costs a dependency and buys nothing.
- **The docstring was checkable and wrong.** Not "documentation drifted" — the
  function was in scope through `optimus-kernel` the whole time. Removing the claim
  is the honest repair; ADR evidence labels exist precisely so a reader can tell
  "Confirmed current behaviour" from a sentence somebody hoped was true.
- **A single catalog is worth having at the scale where it is real.** Within one
  surface, one list that four consumers read is a genuine invariant a test can hold.
  Across surfaces that share three rows, it is a slogan.
- **Predictive typing raises the cost of a stale row.** A list the user reads once
  when they type `/help` tolerates a wrong entry; a list that proposes completions
  as they type puts the wrong entry into their draft.
- **The picker rule is load-bearing, not stylistic.** Modality is the difference
  between a list that helps and a list that swallows Enter.

## Consequences

- Adding a terminal command is one catalog row. Dispatch, menu, help and completion
  follow; `every_catalogued_command_dispatches` fails if the arm is missing.
- `/help` output is generated by `format!("{:<17} {}", command.typed_form(), command.summary)`
  rather than hand-aligned, so summaries can no longer disagree between help and menu.
- One user-visible change: the menu row for `/access` now reads
  `standard|review_changes|read_only|full_project` — the catalog summary — instead of
  the old bespoke `autonomy profile for new turns`. One summary per command is the
  point; the bespoke string was the duplication.
- The terminal still has no `optimus-ops` dependency, and now no comment implying it.
- Tab, Up and Down are contextual in the composer. Up/Down recall history except
  over a lone `/word`; Tab does nothing except over a lone `/word`.
  **Superseded 2026-08-02 by ADR-0075 phase 2** for the second half only: Tab
  still completes over a lone `/word`, and otherwise now hands the keyboard to
  the transcript rather than doing nothing. The precedence recorded here is
  unchanged — the suggestion overlay still borrows exactly Tab, Up and Down,
  and a picker still outranks it.
- `apps/optimus-tui/src/session.rs` sits at 796 of 800 production lines. This change
  added three. The next feature to touch it must split it first; that is a known
  debt recorded here, not a surprise for whoever hits it.

## Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| A future reader re-derives the discarded idea and wires the terminal to `builtin_surface_commands` | Medium | Low | The three-row measurement is recorded above with the query that produced it, so the check is cheap to repeat rather than re-argue |
| The two lists drift into genuine conflict — both define `/new`, differently | Low | Medium | The three shared rows (`help`, `new`, `yolo`) are the ones whose meaning is obvious enough to stay stable; a conflict would surface as a user-visible behaviour difference between surfaces, which is the reportable symptom |
| Suggestions swallow a key the user wanted for something else | Low | Medium | `mode.suggesting` is false unless the whole draft is one `/word`; `the_arrows_pick_a_suggestion_before_they_recall_a_prompt` holds the boundary end to end through a pty |
| The overlay paints over the prompt at some terminal height | Low | High | `the_list_stops_above_the_composer_at_every_height` asserts the bottom border lands exactly one row above the composer for every height 6..16, and that the list stands down entirely where there is no room |
| `session.rs` hits 800 and blocks unrelated work | Medium | Medium | Recorded above and tracked as its own task; the gate fails loudly at land time, not silently |

## Evaluation evidence

- **Confirmed current behaviour.** `optimus_ops::builtin_surface_commands()` returns
  16 rows: `help, doctor, yolo, sessions, new, skills, memory, packs, logs, mail, cron,
  artifacts, capabilities, packs.list, skills.list, memory.recall`.
- **Confirmed current behaviour.** Terminal `COMMANDS` holds 12 names: `access,
  approval, frame, help, model, mouse, new, provider, providers, quit, thinking, yolo`,
  plus `exit` as an alias of `quit`. Intersection with the registry: `help`, `new`, `yolo`.
- **Confirmed current behaviour.**
  `git show c61348c08df7028df0837a09fbb285beb9e002e3:apps/optimus-tui/src/commands.rs`
  contains the registry docstring, no `optimus_ops` import, a 7-tuple menu array and a
  17-line `help()` array. `apps/optimus-tui/Cargo.toml` lists no `optimus-ops` dependency.
- **Confirmed current behaviour.** `cargo test -p optimus-tui --lib` — 213 passed,
  0 failed. `cargo test -p optimus-tui --test pty` — 11 passed, 0 failed.
- **Confirmed current behaviour.** `scripts/gates/check-module-size.py` passes; the largest
  terminal module is `session.rs` at 796 production lines against the hard limit of 800,
  and `apps/optimus-tui/**` has no entry in `docs/architecture/module-size-baseline.json`.
- **Inferred behaviour.** The 81% discard rate is arithmetic on the two lists as they
  stand today; it is not a claim about what either list will contain after further work.

## Conditions for reconsideration

Revisit clause 2 if the terminal grows agent-surface commands — `/sessions`,
`/packs`, `/cron`, `/artifacts` — such that the overlap passes half of the smaller
list. At that point a shared core with a per-surface extension is worth the
dependency, and this ADR should be superseded rather than quietly ignored.

Revisit clause 5 if suggestions ever need a key that history or the composer already
owns. The non-modal design has exactly three keys of headroom; a fourth means
deciding what it takes away, which is a new decision.

## Relevant code

- `apps/optimus-tui/src/commands.rs` — `Command`, `COMMANDS`, `lookup`, `dispatch`,
  `menu`, `help`
- `apps/optimus-tui/src/completion.rs` — `suggestions`, `Completion`
- `apps/optimus-tui/src/keys.rs` — `Mode.suggesting`, `Intent::Suggest`, `Intent::Complete`
- `apps/optimus-tui/src/view.rs` — `draw_suggestions`
- `crates/optimus-ops/src/surface_commands.rs` — the registry this decision declines to import
- `crates/optimus-kernel/src/lib.rs:122` — the re-export that made the old claim checkable

## Relevant tests

- `apps/optimus-tui/tests/pty.rs` — `a_half_typed_command_offers_its_matches_and_tab_takes_one`,
  `the_arrows_pick_a_suggestion_before_they_recall_a_prompt`
- `apps/optimus-tui/src/commands.rs` — `every_catalogued_command_dispatches`,
  `the_menu_only_offers_commands_that_work_from_a_click`
- `apps/optimus-tui/src/completion.rs` — 14 unit tests over `suggestions` and `Completion`
- `apps/optimus-tui/src/view.rs` — `a_half_typed_command_offers_the_names_it_could_still_become`,
  `ordinary_typing_is_never_covered_by_a_list`, `the_highlighted_row_is_the_one_tab_would_take`,
  `the_list_stops_above_the_composer_at_every_height`

## Explicit non-claims

- This does not claim the terminal and the other surfaces offer the same commands.
  They do not, and clause 2 says they need not.
- This does not deprecate `optimus_ops::builtin_surface_commands()` or change what
  the CLI and Desktop read from it. Nothing outside `apps/optimus-tui/**` changes.
- This does not claim predictive typing is complete. Suggestion rows are not
  clickable — the right-click menu already serves mouse users — and completion does
  not extend to arguments, only to command names.
- This does not claim the 800-line limit is comfortable for the terminal. `session.rs`
  at 796 says otherwise, and this ADR records that rather than resolving it.
