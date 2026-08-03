//! A vertical selection list for choosing one option.
//!
//! Kept as plain state with no ratatui types so the movement and selection rules
//! are unit-testable without standing up a terminal. `view` renders it; the event
//! loop routes keys to it while one is open.

/// One selectable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// Value handed back on confirm.
    pub id: String,
    /// Left-hand label.
    pub label: String,
    /// Muted right-hand detail, e.g. connect state.
    pub detail: String,
    /// True for the option already in effect.
    pub current: bool,
    /// False when the option needs a sign-in before it can run a turn.
    pub connected: bool,
}

/// What a confirmed pick applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Provider,
    /// Approve-and-continue / deny for the pending exact approval.
    Approval,
    /// Confirm the break-glass access transition before it reaches the host.
    Yolo,
    /// Open one of the durable sessions shown in the workspace rail.
    Session,
    /// Open one of the pinned durable sessions shown in the workspace rail.
    PinnedSession,
    /// Select an authorized project scope shown in the workspace rail.
    Project,
    /// Slash commands, reachable with the right mouse button.
    Command,
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub kind: PickerKind,
    pub title: String,
    pub items: Vec<PickerItem>,
    selected: usize,
}

impl Picker {
    /// Opens on the current option so Enter is a no-op rather than a surprise.
    pub fn new(kind: PickerKind, title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        let selected = items.iter().position(|item| item.current).unwrap_or(0);
        Self {
            kind,
            title: title.into(),
            items,
            selected,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Point the selection at a row, ignoring one that is not there — a click
    /// lands on a screen cell, which may not be a row at all.
    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
        }
    }

    /// Wraps, because a list you cannot leave from either end is a trap.
    pub fn down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    pub fn up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + self.items.len() - 1) % self.items.len();
    }

    pub fn confirm(&self) -> Option<&PickerItem> {
        self.items.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<PickerItem> {
        vec![
            PickerItem {
                id: "offline".into(),
                label: "offline".into(),
                detail: "ready".into(),
                current: false,
                connected: true,
            },
            PickerItem {
                id: "codex".into(),
                label: "codex".into(),
                detail: "not connected".into(),
                current: true,
                connected: false,
            },
            PickerItem {
                id: "openai_compat".into(),
                label: "openai_compat".into(),
                detail: "not connected".into(),
                current: false,
                connected: false,
            },
        ]
    }

    fn picker() -> Picker {
        Picker::new(PickerKind::Provider, "Provider", items())
    }

    #[test]
    fn it_opens_on_the_option_already_in_effect() {
        assert_eq!(picker().confirm().unwrap().id, "codex");
    }

    #[test]
    fn it_opens_at_the_top_when_nothing_is_current() {
        let mut rows = items();
        rows[1].current = false;
        let picker = Picker::new(PickerKind::Provider, "Provider", rows);
        assert_eq!(picker.confirm().unwrap().id, "offline");
    }

    #[test]
    fn moving_down_and_up_returns_where_it_started() {
        let mut picker = picker();
        picker.down();
        assert_eq!(picker.confirm().unwrap().id, "openai_compat");
        picker.up();
        assert_eq!(picker.confirm().unwrap().id, "codex");
    }

    #[test]
    fn it_wraps_at_both_ends() {
        let mut picker = picker();
        picker.down();
        picker.down();
        assert_eq!(
            picker.confirm().unwrap().id,
            "offline",
            "bottom wraps to top"
        );
        picker.up();
        assert_eq!(
            picker.confirm().unwrap().id,
            "openai_compat",
            "top wraps to bottom"
        );
    }

    #[test]
    fn an_empty_picker_never_panics_or_confirms() {
        let mut picker = Picker::new(PickerKind::Provider, "Provider", Vec::new());
        picker.down();
        picker.up();
        assert!(picker.confirm().is_none());
    }
}
