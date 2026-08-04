---
doc_id: architecture-tui-quality-audit-2026-07
doc_type: history
plane: history
status: historical
authority: historical
summary: The mandate lands on north-star ground that already exists: criterion C4 says the terminal reaches 8 of 22 critical host methods, and the thesis says no surface may lose track of its project — the TUI currently has no project concept at...
reviewed_on: 2026-07-31
review_by: never
---

# Terminal face quality audit (2026-07)

> **Documentary status:** Point-in-time audit at `9c1318d`, driven against the
> real binary (`target/debug/optimus`, tmux pty, offline provider, fresh home).
> The bar is explicit: the terminal face must hold up next to Claude Code and
> Codex CLI — everything works as intended, nothing feels half-baked. Findings
> are labeled per AGENTS.md: **Confirmed** = reproduced live in the pty or
> proven by source; anything weaker says so.

The mandate lands on north-star ground that already exists: criterion **C4**
says the terminal reaches 8 of 22 critical host methods, and the thesis says no
surface may lose track of its project — the TUI currently has no project
concept at all. This audit turns "feels half-baked" into ranked, verifiable
units.

## What already holds up

Confirmed live: turns stream into the transcript with tool rows updating in
place; Ctrl-C cancels a run and exits when idle; approvals park as decision
cards and resolve; provider/model/thinking survive relaunch
(`preferences.rs`); word-boundary wrap at 96 columns rereads well on resize;
mouse wheel/drag scrolling with a surrenderable capture (`/mouse`). The
`TurnUpdate` seam (`session.rs`) is a clean wire shape for any future
transport. The foundation is right; the gaps are around it.

## Ranked defects

### U1 — the composer loses or misfires input (Confirmed, live)

The composer is an append-only `String` (`lib.rs:135-141`). Reproduced in the
pty, in one sitting:

- **A multiline paste submits mid-paste.** Bracketed paste is never enabled,
  so a pasted newline is an Enter: pasting two lines fired line one as a live
  turn and left line two in the composer. On a paid provider this spends
  tokens on half a prompt. This alone breaks the "paste an error message and
  ask" loop that a terminal agent exists for.
- **Readline chords type letters.** Ctrl-A/D/K/U each inserted their raw
  character (composer read `dakuabcZ` after the probe). Every terminal user's
  muscle memory actively corrupts the input.
- **No cursor.** Left/Right/Home/End do nothing; typing after Left appends at
  the end. Editing the middle of a prompt means retyping it.
- **No input history.** Up/Down are dead outside the picker.
- **No multiline composing.** Enter always submits; there is no way to write
  a two-paragraph prompt.
- **Esc exits the whole app when idle** (`lib.rs:133`), silently, from the
  same key that closes a picker. One stray keystroke ends the session — and
  U2 means there is no way back to what was on screen.

### U2 — durable sessions are unreachable from the terminal (Confirmed, live)

Sessions are durable and listable (`optimus sessions`, host `sessions`
method), but the TUI cannot resume one: relaunch always opens a blank new
session; there is no `/sessions`, no `/resume`, no `--continue`, and no
transcript reload (host `get_session` is never called — `session.rs` touches
`sessions` only to recover an id after an approval park). Claude Code and
Codex both treat resume as table stakes. This is also the largest single C4
terminal-column gap: delete/rename/pin/search/get_session are all N/A on this
surface today.

### U3 — the terminal face has no project (Confirmed, source)

The thesis: *no surface may lose track of which project it's in.* The TUI has
no project selection, sends no `project_id` on turns (`turn_params`,
`session.rs:450`), and cannot reach the project-scoped `sessions` view that
criterion C1 just landed. The flagship surface contradicts the flagship
sentence.

### U4 — modes without exits, states without affordances (Confirmed, source)

`/yolo` sets `yolo = true` and has no off (`commands.rs:229-251`); the status
bar shows `YOLO` forever. `/frame` and `/mouse` are toggles; `/yolo` is a
ratchet nobody asked for. Smaller kin: no `/quit` (exit is Esc — see U1), no
Ctrl-L clear, Backspace pops `char`s so multi-scalar graphemes delete
partially.

### U5 — first-run and finish (Confirmed, live)

A fresh launch is an empty box: no name, no version, no "type /help", no hint
that `offline` is a demo provider. Transcript markdown is bold-only
(`transcript.rs`) — fenced code blocks render as body text, which matters for
an agent that answers in code.

## Sequence and enforcement

Fix order **U1 → U2 → U3 → U4 → U5**: input correctness first (it loses user
work), then resume (it strands user work), then project truth (it is the
thesis), then mode hygiene, then polish. Each unit ships with inline tests in
the TUI's existing style — the composer and key-routing are plain state
machines and fully assertable without a terminal, which is what keeps these
fixes from regressing silently. The durable enforcement is C4's planned
terminal column in `check-desktop-ipc-matrix.py`: U2/U3 move real methods out
of N/A, and the counter holds them there.
