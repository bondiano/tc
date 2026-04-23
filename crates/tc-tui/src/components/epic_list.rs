use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::{App, FocusPanel};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let items: Vec<ListItem> = app
        .epics
        .iter()
        .map(|e| {
            let count = app.epic_count(e);
            ListItem::new(format!("{e} ({count})"))
        })
        .collect();

    let focused = app.focus == FocusPanel::Epics;
    let title = if focused { "[ Epics ]" } else { " Epics " };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.selected_epic));
    frame.render_stateful_widget(list, area, &mut state);
}
