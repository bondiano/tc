use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::app::AppScreen;
use crate::components::{
    confirm_delete, create_task_form, dag_view, detail, epic_list, help, input, log_viewer,
    settings, task_card, task_table, which_key,
};

pub fn render(app: &App, frame: &mut Frame<'_>) {
    if app.screen == AppScreen::CreateTask {
        create_task_form::render(app, frame);
        return;
    }

    let size = frame.area();
    let input_rows = input::required_height(app);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(input_rows),
        ])
        .split(size);

    render_header(app, frame, chunks[0]);
    render_body(app, frame, chunks[1]);
    input::render(app, frame, chunks[2]);

    if let Some(task) = &app.pending_delete {
        confirm_delete::render(task, app.confirm_delete_yes, frame);
    }

    if app.show_task_card {
        task_card::render(app, frame);
    }

    if app.show_help {
        help::render(frame);
    }

    if app.settings.is_some() {
        settings::render(app, frame);
    }

    if let Some(chord) = app.which_key_chord() {
        which_key::render(frame, chord);
    }
}

fn render_header(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let title = format!(
        "Task Commander    {}    Epic: {}    Filter: {}",
        app.workers_summary(),
        app.current_epic(),
        if app.filter.is_empty() {
            "-"
        } else {
            app.filter.as_str()
        },
    );
    let p = Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(p, area);
}

fn render_body(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(20),
            Constraint::Length(40),
        ])
        .split(area);

    epic_list::render(app, frame, cols[0]);

    if app.show_log {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(cols[1]);
        task_table::render(app, frame, rows[0]);
        log_viewer::render(app, frame, rows[1]);
    } else {
        task_table::render(app, frame, cols[1]);
    }

    if app.show_dag {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(cols[2]);
        detail::render(app, frame, rows[0]);
        dag_view::render(app, frame, rows[1]);
    } else {
        detail::render(app, frame, cols[2]);
    }
}

#[allow(dead_code)]
fn _placeholder(area: ratatui::layout::Rect, frame: &mut Frame<'_>) {
    frame.render_widget(Block::default().borders(Borders::ALL), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{app_with, dummy_task};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn snapshot_basic_layout() {
        let tasks = vec![
            dummy_task("T-001", "alpha", "todo"),
            dummy_task("T-002", "alpha", "in_progress"),
            dummy_task("T-003", "beta", "done"),
        ];
        let app = app_with(tasks);
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| render(&app, f)).expect("draw");
        let buf = terminal.backend().buffer().clone();
        let mut found_tasks = false;
        let mut found_epic = false;
        'rows: for y in 0..buf.area.height {
            let mut row = String::new();
            'cols: for x in 0..buf.area.width {
                row.push_str(buf[(x, y)].symbol());
                continue 'cols;
            }
            if row.contains("T-001") {
                found_tasks = true;
            }
            if row.contains("alpha") {
                found_epic = true;
            }
            continue 'rows;
        }
        assert!(found_tasks, "expected task ID rendered");
        assert!(found_epic, "expected epic rendered");
    }

    #[test]
    fn snapshot_with_dag_toggle() {
        let tasks = vec![
            dummy_task("T-001", "alpha", "todo"),
            dummy_task("T-002", "alpha", "todo"),
        ];
        let mut app = app_with(tasks);
        app.show_dag = true;
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| render(&app, f)).expect("draw");
    }
}
