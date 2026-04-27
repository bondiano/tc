use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, AppScreen, SmartView};
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
    let palette = &app.palette;
    let mut spans: Vec<Span> = vec![
        Span::styled(
            "Task Commander  ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.workers_summary(), Style::default().fg(palette.muted)),
        Span::raw("  "),
    ];

    // Smart-view tabs (M-7.2). Active tab uses the theme's accent; numeric
    // shortcut is surfaced inline so users can learn the binding from the
    // bar.
    for (i, view) in SmartView::all().iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let active = app.smart_view == *view;
        let style = if active {
            Style::default()
                .fg(palette.tab_active_fg)
                .bg(palette.tab_active_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.tab_inactive)
        };
        spans.push(Span::styled(
            format!(" [{}] {} ", view.shortcut(), view.label()),
            style,
        ));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled("Epic: ", Style::default().fg(palette.muted)));
    spans.push(Span::raw(app.current_epic().to_string()));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("Fuzzy: ", Style::default().fg(palette.muted)));
    if app.filter.is_empty() {
        spans.push(Span::styled("-", Style::default().fg(palette.muted)));
    } else {
        spans.push(Span::styled(
            app.filter.clone(),
            Style::default().fg(palette.accent),
        ));
    }

    let p = Paragraph::new(Line::from(spans));
    frame.render_widget(p, area);
}

/// Pick the layout shape for the body (M-7.7). Two breakpoints:
///   * width < 80  -> single-column stack (Tasks above, Detail below)
///   * width < 100 -> hide the Epics sidebar; keep Tasks + Detail side-by-side
///   * otherwise   -> three-column layout (Epics + Tasks + Detail)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyLayout {
    Stacked,
    NoSidebar,
    Full,
}

pub(crate) fn body_layout_for_width(width: u16) -> BodyLayout {
    if width < 80 {
        BodyLayout::Stacked
    } else if width < 100 {
        BodyLayout::NoSidebar
    } else {
        BodyLayout::Full
    }
}

fn render_body(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    match body_layout_for_width(area.width) {
        BodyLayout::Full => render_body_full(app, frame, area),
        BodyLayout::NoSidebar => render_body_no_sidebar(app, frame, area),
        BodyLayout::Stacked => render_body_stacked(app, frame, area),
    }
}

fn render_body_full(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(20),
            Constraint::Length(40),
        ])
        .split(area);

    epic_list::render(app, frame, cols[0]);
    render_tasks_with_log(app, frame, cols[1]);
    render_detail_with_dag(app, frame, cols[2]);
}

fn render_body_no_sidebar(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(34)])
        .split(area);

    render_tasks_with_log(app, frame, cols[0]);
    render_detail_with_dag(app, frame, cols[1]);
}

fn render_body_stacked(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    // Single-column phone-ish layout: Tasks on top, Detail below. Log & DAG
    // toggles still live within their owning panel so users keep their
    // affordances on narrow terminals.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_tasks_with_log(app, frame, rows[0]);
    render_detail_with_dag(app, frame, rows[1]);
}

fn render_tasks_with_log(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    if app.show_log {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        task_table::render(app, frame, rows[0]);
        log_viewer::render(app, frame, rows[1]);
    } else {
        task_table::render(app, frame, area);
    }
}

fn render_detail_with_dag(app: &App, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
    if app.show_dag {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        detail::render(app, frame, rows[0]);
        dag_view::render(app, frame, rows[1]);
    } else {
        detail::render(app, frame, area);
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

    /// Header must render the four smart-view tabs with their numeric
    /// shortcuts (M-7.2). Failure here usually means a label/shortcut got
    /// out of sync between `SmartView::label/shortcut` and the renderer.
    #[test]
    fn snapshot_header_shows_smart_view_tabs() {
        let app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        let backend = TestBackend::new(120, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut header = String::new();
        'cols: for x in 0..buf.area.width {
            header.push_str(buf[(x, 0)].symbol());
            continue 'cols;
        }
        for needle in ["[1] Today", "[2] Upcoming", "[3] Inbox", "[4] All"] {
            assert!(header.contains(needle), "header missing {needle}: {header}");
        }
    }

    // ── M-7.7 responsive layout ───────────────────────────────────

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let mut out = String::new();
        'rows: for y in 0..buf.area.height {
            'cols: for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
                continue 'cols;
            }
            out.push('\n');
            continue 'rows;
        }
        out
    }

    #[test]
    fn body_layout_breakpoints_pick_the_right_shape() {
        assert_eq!(body_layout_for_width(60), BodyLayout::Stacked);
        assert_eq!(body_layout_for_width(79), BodyLayout::Stacked);
        assert_eq!(body_layout_for_width(80), BodyLayout::NoSidebar);
        assert_eq!(body_layout_for_width(99), BodyLayout::NoSidebar);
        assert_eq!(body_layout_for_width(100), BodyLayout::Full);
        assert_eq!(body_layout_for_width(140), BodyLayout::Full);
    }

    /// At narrow widths the Epics sidebar must collapse so the task list
    /// gets the breathing room (M-7.7).
    #[test]
    fn snapshot_collapses_sidebar_under_100_cols() {
        let app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        let backend = TestBackend::new(90, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("T-001"), "task row should still render");
        assert!(text.contains("Detail"), "detail panel should still render");
        assert!(
            !text.contains("Epics"),
            "Epics panel must disappear under 100 cols:\n{text}"
        );
    }

    /// At very narrow widths, panels stack vertically so each one stays
    /// readable (M-7.7).
    #[test]
    fn snapshot_stacks_panels_under_80_cols() {
        let app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text = buffer_text(&buf);

        assert!(text.contains("T-001"));
        assert!(text.contains("Detail"));
        assert!(!text.contains("Epics"));

        // In the stacked layout, the Detail panel header must appear on a
        // row strictly below the Tasks header. We can't easily probe per-row
        // contents, but we can locate the substring offsets.
        let tasks_at = text.find("Tasks").expect("Tasks header rendered");
        let detail_at = text.find("Detail").expect("Detail header rendered");
        assert!(
            detail_at > tasks_at,
            "Detail must render below Tasks in stacked mode (tasks={tasks_at}, detail={detail_at})"
        );
    }

    /// The default 100+ wide layout must still show all three panels.
    #[test]
    fn snapshot_full_layout_shows_all_three_panels() {
        let app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        let backend = TestBackend::new(120, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Epics"), "epics panel missing");
        assert!(text.contains("Tasks"), "tasks panel missing");
        assert!(text.contains("Detail"), "detail panel missing");
    }

    // ── M-7.10 snapshots for the remaining redesigned surfaces ────────

    /// Active fuzzy filter must surface in the header so the user knows
    /// the task list is being narrowed (M-7.1).
    #[test]
    fn snapshot_header_shows_active_fuzzy_filter() {
        let mut app = app_with(vec![
            dummy_task("T-001", "alpha", "todo"),
            dummy_task("T-002", "alpha", "in_progress"),
        ]);
        app.filter = "alpha".into();
        let backend = TestBackend::new(120, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut header = String::new();
        'cols: for x in 0..buf.area.width {
            header.push_str(buf[(x, 0)].symbol());
            continue 'cols;
        }
        assert!(
            header.contains("Fuzzy:") && header.contains("alpha"),
            "header should display active fuzzy query: {header}"
        );
    }

    /// While the user is typing into the fuzzy filter, the bottom input
    /// row must show the prompt + buffered query (M-7.1).
    #[test]
    fn snapshot_fuzzy_input_prompt_visible() {
        use crate::app::InputMode;
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::Filter;
        app.input.set_text("al");
        app.filter = "al".into();
        let backend = TestBackend::new(100, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("Filter") || text.contains("Fuzzy"),
            "input mode prompt missing:\n{text}"
        );
        assert!(text.contains("al"), "buffered query missing:\n{text}");
    }

    /// Fullscreen create-task modal (M-7.6) must take over the whole
    /// screen and surface its hint bar.
    #[test]
    fn snapshot_fullscreen_create_task_modal() {
        use crate::app::AppScreen;
        use crate::create_task::CreateTaskForm;
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.create_task_form = Some(CreateTaskForm::new("alpha".into()));
        app.screen = AppScreen::CreateTask;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("Create Task"),
            "expected modal title:\n{text}"
        );
        assert!(
            text.contains("Title") && text.contains("Epic"),
            "expected core form fields:\n{text}"
        );
        assert!(
            text.contains("Ctrl+S") || text.contains("[Esc]"),
            "expected keybinding hints in modal:\n{text}"
        );
        // The main 3-pane layout must be hidden -- the modal is fullscreen.
        assert!(
            !text.contains("Epics ("),
            "expected base layout hidden:\n{text}"
        );
    }

    /// Edit-mode of the same modal should display the editing task ID in
    /// its hint bar.
    #[test]
    fn snapshot_fullscreen_edit_modal_shows_id() {
        use crate::app::AppScreen;
        use crate::create_task::CreateTaskForm;
        let task = dummy_task("T-042", "alpha", "todo");
        let mut app = app_with(vec![task.clone()]);
        app.create_task_form = Some(CreateTaskForm::from_task(&task));
        app.screen = AppScreen::CreateTask;
        let backend = TestBackend::new(120, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("Edit Task") && text.contains("T-042"),
            "expected edit hint with task id:\n{text}"
        );
    }

    /// Completion animation (M-7.8) must apply CROSSED_OUT to the row
    /// while the timer is running. We don't try to introspect terminal
    /// styles per cell here -- instead we verify the App-level helpers
    /// expose progress and the row text still renders.
    #[test]
    fn completion_animation_marks_row_styling() {
        use ratatui::style::Modifier;
        use tc_core::task::TaskId;

        let task = dummy_task("T-007", "alpha", "todo");
        let mut app = app_with(vec![task.clone()]);
        app.mark_completed(&TaskId("T-007".into()));

        let progress = app.completion_progress(&TaskId("T-007".into()));
        assert!(
            progress.is_some(),
            "freshly completed task should have animation progress"
        );

        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Find the row containing T-007 and assert that *some* cell on it
        // carries CROSSED_OUT -- the renderer applies the row-level style
        // overlay only while the animation is active.
        let mut crossed_out_seen = false;
        'rows: for y in 0..buf.area.height {
            let mut row_text = String::new();
            'cols: for x in 0..buf.area.width {
                row_text.push_str(buf[(x, y)].symbol());
                continue 'cols;
            }
            if !row_text.contains("T-007") {
                continue 'rows;
            }
            'cells: for x in 0..buf.area.width {
                if buf[(x, y)].modifier.contains(Modifier::CROSSED_OUT) {
                    crossed_out_seen = true;
                    break 'cells;
                }
                continue 'cells;
            }
        }
        assert!(
            crossed_out_seen,
            "expected CROSSED_OUT modifier on an animated row"
        );
    }

    /// Once the animation duration has elapsed, the row must render in
    /// its baseline style. We can't sleep in tests without flakiness, so
    /// we exercise `prune_completion_animations` directly.
    #[test]
    fn completion_animation_expires_via_prune() {
        use std::time::{Duration, Instant};
        use tc_core::task::TaskId;

        let mut app = app_with(vec![dummy_task("T-007", "alpha", "todo")]);
        let id = TaskId("T-007".into());
        // Insert with a start time well in the past so the prune call
        // drops the entry without waiting.
        app.completion_animations
            .insert(id.clone(), Instant::now() - Duration::from_secs(60));
        app.prune_completion_animations();
        assert!(app.completion_progress(&id).is_none());
        assert!(app.animation_wake_in().is_none());
    }
}
