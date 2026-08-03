//! Shared visual treatment for transient workbench overlays.
//!
//! Pickers and live suggestions have different input ownership, but they are
//! still part of one visual system: a modal surface gets a quiet backdrop,
//! every overlay has an accent, and the underlying frame is cleared before a
//! panel is painted. Keeping that vocabulary here prevents each new viewer or
//! menu from inventing a slightly different box.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};

use crate::bordered;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A picker owns the keyboard and pointer until it closes.
    Modal,
    /// Suggestions remain attached to the draft and do not suspend the frame.
    Suggestions,
}

fn accent(kind: Kind) -> Color {
    match kind {
        Kind::Modal => Color::LightYellow,
        Kind::Suggestions => Color::LightCyan,
    }
}

/// A selected row should read as part of the workbench, not as a terminal
/// copy-selection rectangle. `REVERSED` turns the entire row white on many
/// terminals and makes a picker feel like an accidental text selection.
pub(crate) fn selection_style(kind: Kind) -> Style {
    let background = match kind {
        Kind::Modal => Color::Rgb(35, 39, 53),
        Kind::Suggestions => Color::Rgb(31, 42, 52),
    };
    Style::default()
        .fg(Color::Rgb(242, 242, 242))
        .bg(background)
        .add_modifier(Modifier::BOLD)
}

/// Paint the part of the frame an overlay will own.
pub(crate) fn prepare(frame: &mut Frame, rect: Rect, kind: Kind) {
    if kind == Kind::Modal {
        let area = frame.area();
        // A dimmed frame makes the active surface obvious without changing the
        // underlying transcript or implying that it is no longer there.
        frame.render_widget(
            Block::default().style(Style::default().add_modifier(Modifier::DIM)),
            area,
        );
    }
    frame.render_widget(Clear, rect);
}

/// The shared panel frame, with a different accent for modal and inline work.
pub(crate) fn panel(title: &str, kind: Kind) -> Block<'_> {
    bordered(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent(kind)))
        .title_style(
            Style::default()
                .fg(accent(kind))
                .add_modifier(Modifier::BOLD),
        )
}
