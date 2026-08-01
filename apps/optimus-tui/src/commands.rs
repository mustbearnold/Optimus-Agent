//! Slash commands for the terminal face.
//!
//! Anything typed starting with `/` is handled here instead of being sent to the
//! model. [`COMMANDS`] is the catalog, and it is load-bearing: `dispatch`
//! refuses a name the catalog does not hold, while `help`, `menu`, and the live
//! suggestions in [`crate::completion`] all render from it. Offering and running
//! therefore cannot drift apart — a row on screen is a command that works.
//!
//! The catalog is the terminal's own, deliberately not `optimus-ops`'
//! `builtin_surface_commands` (ADR-0074). The shared registry addresses
//! surfaces that dispatch work through the host; these address the session this
//! process is holding open. Sixteen rows there, twelve here, and three names in
//! both — sourcing one from the other would discard most of its input.
//!
//! Commands answer locally and synchronously. Nothing here starts a turn, so the
//! screen thread is never blocked by one.

use optimus_host::handle_ipc;
use serde_json::json;

use crate::picker::{Picker, PickerItem, PickerKind};
use crate::session::{Role, TuiSession};
use crate::transcript::Chrome;

/// One slash command, as it is offered and as it is run.
pub struct Command {
    pub name: &'static str,
    /// Placeholder for the argument, for the commands that read one.
    pub argument: Option<&'static str>,
    /// One line, short enough to sit beside the name in a narrow terminal.
    pub summary: &'static str,
    /// Offered by the right-click menu. See [`offered`] for what earns a row.
    pub menu: bool,
    /// Other spellings that reach the same arm. Never offered, so `/exit` does
    /// not take a second row beside the `/quit` that does the same thing.
    pub aliases: &'static [&'static str],
}

impl Command {
    /// The command as it is typed, argument placeholder included.
    pub fn typed_form(&self) -> String {
        match self.argument {
            Some(argument) => format!("/{} {argument}", self.name),
            None => format!("/{}", self.name),
        }
    }
}

/// A command the right-click menu offers.
///
/// It has to be worth clicking: the bare form must do something, so a row that
/// then demanded typing is never a worse path than typing the command was. That
/// rules out `/model` and friends, and two that are bare but still excluded —
/// `/mouse` would take away the very menu that ran it, and `/quit` is not a
/// thing a stray click should reach.
const fn offered(name: &'static str, summary: &'static str) -> Command {
    Command {
        name,
        argument: None,
        summary,
        menu: true,
        aliases: &[],
    }
}

/// A command you reach by typing it, with the suggestions to help.
const fn typed(
    name: &'static str,
    argument: Option<&'static str>,
    summary: &'static str,
) -> Command {
    Command {
        name,
        argument,
        summary,
        menu: false,
        aliases: &[],
    }
}

/// Every command the terminal answers.
///
/// Ordered by how often a session reaches for one rather than alphabetically:
/// the menu and the suggestion list are both shortcuts, and alphabetical order
/// serves a name you already know — in which case you would have typed it.
pub const COMMANDS: &[Command] = &[
    offered("providers", "pick a provider from a list"),
    typed(
        "provider",
        Some("<id>"),
        "switch provider, remembered next launch",
    ),
    typed("model", Some("<id>"), "override the model, or 'auto'"),
    typed(
        "thinking",
        Some("<lvl>"),
        "minimal|low|medium|high|xhigh|max, or off",
    ),
    offered("approval", "decide the pending exact approval"),
    // Takes a profile but reads back the current one bare, so it earns a row.
    Command {
        name: "access",
        argument: Some("<profile>"),
        summary: "standard|review_changes|read_only|full_project",
        menu: true,
        aliases: &[],
    },
    offered("yolo", "unrestricted access, releases the open approval"),
    offered("frame", "containers around turns, or plain gutters"),
    typed(
        "mouse",
        None,
        "hand the mouse back to the terminal, or take it",
    ),
    offered("new", "start a fresh session"),
    Command {
        name: "quit",
        argument: None,
        summary: "leave Optimus",
        menu: false,
        aliases: &["exit"],
    },
    offered("help", "list every command"),
];

/// The catalog entry for `name`, by its own spelling or one of its aliases.
pub fn lookup(name: &str) -> Option<&'static Command> {
    COMMANDS
        .iter()
        .find(|command| command.name == name || command.aliases.contains(&name))
}

/// Handle `input` if it is a slash command. Returns false for ordinary prompts.
pub fn dispatch(session: &mut TuiSession, input: &str) -> bool {
    let trimmed = input.trim();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut parts = rest.split_whitespace();
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let argument = parts.next().unwrap_or("").to_string();

    if name.is_empty() {
        session.push(Role::Error, "type /help for commands".into());
        return true;
    }
    // The catalog decides what exists; the match below decides what it does. A
    // name the catalog does not hold never reaches the match, so a command
    // added to one and forgotten in the other cannot half-work.
    let Some(command) = lookup(&name) else {
        session.push(Role::Error, format!("unknown command /{name} — try /help"));
        return true;
    };
    match command.name {
        "help" => help(session),
        "providers" => providers(session),
        "provider" => set_provider(session, &argument),
        "model" => set_model(session, &argument),
        "thinking" => set_thinking(session, &argument),
        "approval" => approval(session),
        "access" => access(session, &argument),
        "yolo" => yolo(session),
        "frame" => frame(session),
        "mouse" => mouse(session),
        "new" => new_session(session),
        "quit" => session.quit = true,
        // Unreachable while the catalog and the arms above agree, which
        // `every_catalogued_command_dispatches` is what holds.
        other => session.push(Role::Error, format!("/{other} is listed but not wired")),
    }
    true
}

/// Commands offered by the right-click menu, in catalog order.
pub fn menu() -> Picker {
    let items = COMMANDS
        .iter()
        .filter(|command| command.menu)
        .map(|command| PickerItem {
            id: command.name.to_string(),
            label: format!("/{}", command.name),
            detail: command.summary.to_string(),
            current: false,
            connected: true,
        })
        .collect();
    Picker::new(PickerKind::Command, "Commands", items)
}

fn help(session: &mut TuiSession) {
    let mut lines: Vec<String> = COMMANDS
        .iter()
        .map(|command| format!("{:<17} {}", command.typed_form(), command.summary))
        .collect();
    lines.extend(
        [
            "Tab completes a half-typed command; Up/Down pick from the suggestions.",
            "Ctrl-C stops a run, or exits when idle; Ctrl-D exits on an empty prompt.",
            "Esc clears the prompt. Shift-Enter (or Ctrl-J) adds a line; Enter sends.",
            "Up/Down recall past prompts; Ctrl-A/E, Alt-arrows, Ctrl-K/U/W edit the line.",
            "PageUp/PageDown scroll the transcript; End follows the tail.",
            "Wheel scrolls, the right border drags, right-click opens this menu.",
        ]
        .map(str::to_string),
    );
    session.push(Role::Assistant, lines.join("\n"));
}

fn approval(session: &mut TuiSession) {
    if session.pending_approval.is_none() {
        session.push(Role::Error, "no approval is pending".into());
        return;
    }
    session.open_approval_picker();
}

fn providers(session: &mut TuiSession) {
    match handle_ipc(&session.home, "providers_catalog", json!({})) {
        Ok(value) => {
            let rows = value
                .get("providers")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut items = vec![PickerItem {
                id: "auto".into(),
                label: "Auto  (recommended)".into(),
                detail: "best available provider".into(),
                current: session.provider == "auto",
                connected: true,
            }];
            items.extend(rows.iter().map(|row| {
                let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let model = row
                    .get("default_model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let connect = row.get("connect").and_then(|v| v.as_str()).unwrap_or("?");
                PickerItem {
                    id: id.to_string(),
                    label: format!("{id}  ({model})"),
                    detail: connect.to_string(),
                    current: id == session.provider,
                    connected: connect.eq_ignore_ascii_case("connected"),
                }
            }));
            if items.is_empty() {
                session.push(Role::Error, "no providers in the catalog".into());
                return;
            }
            session.picker = Some(Picker::new(
                PickerKind::Provider,
                "Select a provider",
                items,
            ));
        }
        Err(error) => session.push(Role::Error, error),
    }
}

fn set_provider(session: &mut TuiSession, argument: &str) {
    if argument.is_empty() {
        session.push(Role::Error, "usage: /provider <id> — see /providers".into());
        return;
    }
    let mut known: Vec<String> = handle_ipc(&session.home, "providers_catalog", json!({}))
        .ok()
        .and_then(|value| value.get("providers").and_then(|v| v.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|row| row.get("id").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    known.insert(0, "auto".into());

    if argument.eq_ignore_ascii_case("auto") && session.model.is_some() {
        session.push(
            Role::Error,
            "choose model Auto before returning provider selection to Auto".into(),
        );
        return;
    }
    if !known.is_empty() && !known.iter().any(|id| id == argument) {
        session.push(
            Role::Error,
            format!("unknown provider {argument} — have: {}", known.join(", ")),
        );
        return;
    }
    session.provider = argument.to_string();
    session.remember_model_choice();
    session.push(
        Role::Assistant,
        format!("provider is now {argument} — remembered for next launch"),
    );
}

/// `/model <id>` — override the selected model, or `auto` to stop overriding.
///
/// Not validated against a catalog the way `/provider` is: model ids come and
/// go faster than any list Optimus holds, and refusing a real id because a
/// stale catalog has not heard of it is the worse failure.
fn set_model(session: &mut TuiSession, argument: &str) {
    match argument {
        "" => session.push(
            Role::Error,
            "usage: /model <id>, or /model auto to let Optimus choose".into(),
        ),
        choice
            if ["auto", "default", "reset", "none"]
                .iter()
                .any(|value| choice.eq_ignore_ascii_case(value)) =>
        {
            session.model = None;
            session.remember_model_choice();
            session.push(Role::Assistant, "model selection is now Auto".into());
        }
        id => {
            if session.provider.eq_ignore_ascii_case("auto") {
                session.push(
                    Role::Error,
                    "choose a provider before setting an explicit model".into(),
                );
                return;
            }
            session.model = Some(id.to_string());
            session.remember_model_choice();
            session.push(
                Role::Assistant,
                format!("model is now {id} — remembered for next launch"),
            );
        }
    }
}

/// `/thinking <level>` — how much reasoning effort to ask for.
///
/// Levels are the backend's, normalised kernel-side; this only rejects what is
/// certainly not one, so a new level added upstream keeps working here.
fn set_thinking(session: &mut TuiSession, argument: &str) {
    const LEVELS: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max"];
    match argument {
        "" => session.push(
            Role::Error,
            format!("usage: /thinking <{}|off>", LEVELS.join("|")),
        ),
        "off" | "none" | "default" => {
            session.thinking = None;
            session.remember_model_choice();
            session.push(Role::Assistant, "thinking level left to the backend".into());
        }
        level if LEVELS.contains(&level) => {
            session.thinking = Some(level.to_string());
            session.remember_model_choice();
            session.push(
                Role::Assistant,
                format!("thinking is now {level} — remembered for next launch"),
            );
        }
        other => session.push(
            Role::Error,
            format!("unknown level {other} — have: {}, off", LEVELS.join(", ")),
        ),
    }
}

/// Closed map from user words to canonical wire profiles. Mirrors the
/// non-break-glass rows of the graph parser; /access deliberately cannot
/// reach UnrestrictedHost — an ordinary-sounding word must not hand over the
/// machine (#118), so break-glass stays /yolo alone.
fn parse_access(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "standard" | "std" => Some("standard"),
        "review_changes" | "review" | "ask" => Some("review_changes"),
        "read_only" | "readonly" | "read" => Some("read_only"),
        "full_project" | "full-project" | "project_full" => Some("full_project"),
        _ => None,
    }
}

const ACCESS_OPTIONS: &str = "standard, review_changes, read_only, full_project";

fn access(session: &mut TuiSession, argument: &str) {
    if argument.is_empty() {
        let current = if session.yolo {
            "unrestricted (via /yolo)"
        } else {
            session.access.unwrap_or("review_changes (default)")
        };
        session.push(
            Role::Assistant,
            format!("access is {current} — /access <{ACCESS_OPTIONS}> changes new turns"),
        );
        return;
    }
    if let Some(profile) = parse_access(argument) {
        // Selecting a bounded profile always narrows out of break-glass; the
        // reverse direction still requires the unmistakable /yolo word.
        let dropped_yolo = std::mem::take(&mut session.yolo);
        session.access = Some(profile);
        let note = if dropped_yolo {
            " (yolo no longer rides new turns)"
        } else {
            ""
        };
        session.push(Role::Assistant, format!("new turns run as {profile}{note}"));
        return;
    }
    let lowered = argument.to_ascii_lowercase();
    let text = if matches!(
        lowered.as_str(),
        "unrestricted" | "unrestricted_host" | "yolo" | "full" | "host"
    ) {
        format!("break-glass is /yolo only; /access offers {ACCESS_OPTIONS}")
    } else {
        format!("unknown profile '{argument}' — /access offers {ACCESS_OPTIONS}")
    };
    session.push(Role::Error, text);
}

fn yolo(session: &mut TuiSession) {
    // Reuse the runtime path the CLI flag uses, so the receipt story is identical.
    match handle_ipc(&session.home, "approvals_release_yolo", json!({})) {
        Ok(value) => {
            let released = value.get("released").and_then(|v| v.as_u64()).unwrap_or(0);
            session.yolo = true;
            session.push(
                Role::Assistant,
                format!(
                    "unrestricted access on. Released {released} open approval(s); each one \
                     recorded a receipt naming the yolo actor."
                ),
            );
        }
        Err(error) => {
            // The method is not wired yet; say so rather than imply it worked.
            session.push(
                Role::Error,
                format!("yolo could not reach the runtime: {error}"),
            );
        }
    }
}

/// Swap between containers and bare gutters.
///
/// Containers are easier to scan; plain is easier to copy out of, because a
/// mouse selection over a box drags the border characters with it.
fn frame(session: &mut TuiSession) {
    let (next, told) = match session.chrome {
        Chrome::Boxed => (Chrome::Plain, "plain gutters — copies cleanly"),
        Chrome::Plain => (Chrome::Boxed, "containers around each turn"),
    };
    session.chrome = next;
    session.push(Role::Assistant, format!("frame: {told}"));
}

/// Take the mouse, or give it back.
///
/// While it is captured the terminal cannot do its own click-and-drag text
/// selection, so anyone who wants to copy a transcript needs a way out.
fn mouse(session: &mut TuiSession) {
    session.mouse = !session.mouse;
    session.push(
        Role::Assistant,
        if session.mouse {
            "mouse on — wheel scrolls, the right border drags, right-click opens the menu".into()
        } else {
            "mouse off — the terminal has it back for selecting and copying".to_string()
        },
    );
}

fn new_session(session: &mut TuiSession) {
    session.session_id = None;
    session.messages.clear();
    session.workbench.clear();
    session.scroll_back = 0;
    // The parked job stays durable and resolvable runtime-side; this surface
    // just stops offering a card bound to the session it left.
    session.pending_approval = None;
    session.push(Role::Assistant, "started a fresh session".into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn session() -> (tempfile::TempDir, TuiSession) {
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        (dir, TuiSession::new(home))
    }

    #[test]
    fn ordinary_text_is_not_a_command() {
        let (_dir, mut session) = session();
        assert!(!dispatch(&mut session, "search the web for ai news"));
        assert!(session.messages.is_empty());
    }

    #[test]
    fn help_lists_the_commands_that_exist() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/help"));
        let text = &session.messages[0].text;
        for expected in [
            "/providers",
            "/provider <id>",
            "/approval",
            "/access <profile>",
            "/yolo",
            "/new",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn access_sets_a_bounded_profile_for_new_turns() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/access standard"));
        assert_eq!(session.access, Some("standard"));
        assert_eq!(session.messages[0].role, Role::Assistant);
        assert!(session.messages[0].text.contains("standard"));
    }

    #[test]
    fn access_falls_closed_on_unknown_words() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/access banana"));
        assert_eq!(session.access, None, "an unknown word must change nothing");
        assert_eq!(session.messages[0].role, Role::Error);
    }

    #[test]
    fn access_cannot_reach_break_glass() {
        let (_dir, mut session) = session();
        for word in ["unrestricted", "unrestricted_host", "yolo", "full", "host"] {
            assert!(dispatch(&mut session, &format!("/access {word}")));
            assert_eq!(session.access, None, "{word} must not select a profile");
            assert!(!session.yolo, "{word} must not enter break-glass");
            let last = session.messages.last().expect("a refusal message");
            assert_eq!(last.role, Role::Error);
            assert!(
                last.text.contains("/yolo"),
                "refusal for {word} names /yolo"
            );
        }
    }

    #[test]
    fn access_narrows_out_of_yolo() {
        let (_dir, mut session) = session();
        session.yolo = true;
        assert!(dispatch(&mut session, "/access review"));
        assert_eq!(session.access, Some("review_changes"));
        assert!(
            !session.yolo,
            "choosing a bounded profile leaves break-glass"
        );
    }

    #[test]
    fn bare_access_reports_without_changing_anything() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/access"));
        assert_eq!(session.access, None);
        assert_eq!(session.messages[0].role, Role::Assistant);
        assert!(session.messages[0]
            .text
            .contains("review_changes (default)"));
    }

    #[test]
    fn approval_with_nothing_pending_says_so() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/approval"));
        assert_eq!(session.messages[0].role, Role::Error);
        assert!(session.messages[0].text.contains("no approval"));
        assert!(session.picker.is_none());
    }

    #[test]
    fn approval_reopens_the_decision_picker_for_the_pending_card() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(crate::session::approval_binding_fixture());
        assert!(dispatch(&mut session, "/approval"));
        let picker = session.picker.as_ref().expect("picker opened");
        assert_eq!(picker.kind, crate::picker::PickerKind::Approval);
        assert_eq!(picker.items[0].id, "approve");
        assert_eq!(picker.items[1].id, "deny");
        assert!(
            picker.items[0].detail.contains("Write src/proof.txt"),
            "the exact action summary is on the approve row"
        );
    }

    #[test]
    fn every_catalogued_command_dispatches() {
        // The catalog is what `help`, the menu, and the live suggestions all
        // offer. An entry with no arm behind it would be a name the terminal
        // advertises and then refuses — the one failure this design exists to
        // make impossible, so it is asserted rather than assumed.
        for command in COMMANDS {
            for spelling in std::iter::once(&command.name).chain(command.aliases) {
                let (_dir, mut session) = session();
                assert!(dispatch(&mut session, &format!("/{spelling}")));
                let refused = session
                    .messages
                    .iter()
                    .any(|m| m.text.contains("not wired") || m.text.contains("unknown command"));
                assert!(!refused, "/{spelling} is offered but does not reach an arm");
            }
        }
    }

    #[test]
    fn the_menu_only_offers_commands_that_work_from_a_click() {
        assert!(!menu().items.is_empty(), "the menu must offer something");
        for item in &menu().items {
            let command = lookup(&item.id).expect("a menu row names a real command");
            let (_dir, mut session) = session();
            assert!(dispatch(&mut session, &format!("/{}", item.id)));
            // A row that reads an argument still has to do something useful
            // without one, or clicking it is a worse path than typing was.
            if command.argument.is_some() {
                assert!(
                    !session
                        .messages
                        .first()
                        .is_some_and(|m| m.role == Role::Error),
                    "/{} demands typing after the click",
                    item.id
                );
            }
        }
    }

    #[test]
    fn choosing_a_menu_row_runs_that_command() {
        let (_dir, mut session) = session();
        session.picker = Some(menu());
        let index = menu().items.iter().position(|i| i.id == "frame").unwrap();
        session.picker.as_mut().unwrap().select(index);
        session.confirm_picker();
        assert_eq!(session.chrome, Chrome::Plain, "the click took effect");
        assert!(session.picker.is_none(), "choosing closes the menu");
    }

    #[test]
    fn frame_returns_to_where_it_started_after_two_toggles() {
        let (_dir, mut session) = session();
        let first = session.chrome;
        dispatch(&mut session, "/frame");
        assert_ne!(session.chrome, first);
        dispatch(&mut session, "/frame");
        assert_eq!(session.chrome, first);
    }

    #[test]
    fn mouse_can_be_handed_back_to_the_terminal_for_copying() {
        let (_dir, mut session) = session();
        assert!(session.mouse, "captured by default");
        dispatch(&mut session, "/mouse");
        assert!(!session.mouse);
        assert!(session.messages[0].text.contains("selecting and copying"));
        dispatch(&mut session, "/mouse");
        assert!(session.mouse, "and taken back again");
    }

    #[test]
    fn new_drops_the_pending_card_with_the_session_it_belonged_to() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(crate::session::approval_binding_fixture());
        dispatch(&mut session, "/new");
        assert!(session.pending_approval.is_none());
    }

    #[test]
    fn providers_opens_a_picker_on_the_active_provider() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/providers"));
        let picker = session.picker.as_ref().expect("picker opened");
        assert!(
            picker.items.iter().any(|i| i.id == "offline"),
            "offline must be selectable"
        );
        assert_eq!(
            picker.confirm().unwrap().id,
            "auto",
            "it opens on the provider already in effect"
        );
    }

    #[test]
    fn confirming_the_picker_switches_provider_and_closes_it() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/providers");
        let picker = session.picker.as_mut().unwrap();
        picker.down();
        let chosen = picker.confirm().unwrap().id.clone();
        session.confirm_picker();
        assert!(session.picker.is_none(), "confirming closes the picker");
        assert_eq!(session.provider, chosen);
    }

    #[test]
    fn confirming_the_unchanged_row_is_silent() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/providers");
        session.confirm_picker();
        assert_eq!(session.provider, "auto");
        assert!(
            session.messages.is_empty(),
            "re-picking the active provider should not announce a change"
        );
    }

    #[test]
    fn the_picker_reports_which_providers_need_a_sign_in() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/providers");
        let picker = session.picker.as_ref().unwrap();
        let offline = picker.items.iter().find(|i| i.id == "offline").unwrap();
        assert!(offline.connected, "offline needs no credential");
        let codex = picker.items.iter().find(|i| i.id == "codex").unwrap();
        assert!(
            !codex.connected,
            "codex has no credential in a fresh home, so it must read as not connected"
        );
    }

    #[test]
    fn picking_an_unconnected_non_oauth_provider_says_so_rather_than_pretending() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/providers");
        let picker = session.picker.as_mut().unwrap();
        let index = picker
            .items
            .iter()
            .position(|i| i.id == "open-ai-compat")
            .expect("open-ai-compat listed");
        while picker.selected() != index {
            picker.down();
        }
        session.confirm_picker();
        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::Error);
        assert!(
            last.text.contains("not connected"),
            "selecting must not imply a working provider: {}",
            last.text
        );
    }

    #[test]
    fn switching_provider_changes_the_next_turn() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/provider codex");
        assert_eq!(session.provider, "codex");
    }

    #[test]
    fn an_unknown_provider_is_refused_with_the_real_list() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/provider nope");
        assert_eq!(session.provider, "auto", "provider must not change");
        assert_eq!(session.messages[0].role, Role::Error);
        assert!(session.messages[0].text.contains("have:"));
    }

    #[test]
    fn provider_without_an_argument_explains_itself() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/provider");
        assert_eq!(session.messages[0].role, Role::Error);
        assert!(session.messages[0].text.contains("/providers"));
    }

    #[test]
    fn an_unknown_command_says_so_instead_of_prompting_the_model() {
        let (_dir, mut session) = session();
        assert!(dispatch(&mut session, "/wat"));
        assert_eq!(session.messages[0].role, Role::Error);
        assert!(session.messages[0].text.contains("/wat"));
    }

    #[test]
    fn new_clears_the_transcript_and_the_session_id() {
        let (_dir, mut session) = session();
        session.session_id = Some("abc".into());
        session.push(Role::User, "old".into());
        dispatch(&mut session, "/new");
        assert!(session.session_id.is_none());
        assert_eq!(session.messages.len(), 1);
    }

    #[test]
    fn a_model_choice_outlives_the_process() {
        let (_dir, mut session) = session();
        let home = session.home.clone();
        dispatch(&mut session, "/provider codex");
        dispatch(&mut session, "/model gpt-5");
        dispatch(&mut session, "/thinking high");

        let relaunched = TuiSession::new(home);
        assert_eq!(relaunched.model.as_deref(), Some("gpt-5"));
        assert_eq!(relaunched.thinking.as_deref(), Some("high"));
    }

    #[test]
    fn returning_to_the_provider_default_is_remembered_too() {
        // Clearing an override is a choice like any other. Persisting only the
        // set case would resurrect a model the user just dismissed.
        let (_dir, mut session) = session();
        let home = session.home.clone();
        dispatch(&mut session, "/provider codex");
        dispatch(&mut session, "/model gpt-5");
        dispatch(&mut session, "/model default");

        assert_eq!(TuiSession::new(home).model, None);
    }

    #[test]
    fn model_auto_clears_and_persists_the_override() {
        let (_dir, mut session) = session();
        let home = session.home.clone();
        dispatch(&mut session, "/provider codex");
        dispatch(&mut session, "/model gpt-5");
        dispatch(&mut session, "/model Auto");

        assert_eq!(session.model, None);
        assert_eq!(TuiSession::new(home).model, None);
    }

    #[test]
    fn auto_provider_does_not_silently_discard_an_explicit_model() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/provider codex");
        dispatch(&mut session, "/model gpt-5");
        dispatch(&mut session, "/provider auto");

        assert_eq!(session.provider, "codex");
        assert_eq!(session.model.as_deref(), Some("gpt-5"));
        assert_eq!(session.messages.last().unwrap().role, Role::Error);
    }

    #[test]
    fn an_unknown_thinking_level_changes_nothing() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/thinking high");
        dispatch(&mut session, "/thinking louder");
        assert_eq!(
            session.thinking.as_deref(),
            Some("high"),
            "a rejected level must not silently clear the good one"
        );
        assert_eq!(session.messages.last().unwrap().role, Role::Error);
    }

    #[test]
    fn thinking_off_is_a_level_not_a_typo() {
        let (_dir, mut session) = session();
        dispatch(&mut session, "/thinking max");
        dispatch(&mut session, "/thinking off");
        assert_eq!(session.thinking, None);
        assert_eq!(session.messages.last().unwrap().role, Role::Assistant);
    }
}
