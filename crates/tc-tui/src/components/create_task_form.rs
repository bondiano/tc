use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::create_task::{CreateTaskField, CreateTaskForm};

/// Returns the clickable area for each form field given the full frame area.
/// Mirrors the layout computed in `render` so click handling stays in sync.
pub fn compute_field_areas(
    frame_area: Rect,
    form: &CreateTaskForm,
) -> Vec<(CreateTaskField, Rect)> {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(frame_area);

    let body = outer[1];
    let sections = body_sections(body, form);

    let row2_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(sections[2]);

    vec![
        (CreateTaskField::Title, sections[0]),
        (CreateTaskField::Epic, row2_cols[0]),
        (CreateTaskField::Priority, row2_cols[1]),
        (CreateTaskField::Assignee, row2_cols[2]),
        (CreateTaskField::DependsOn, sections[4]),
        (CreateTaskField::AcceptanceCriteria, sections[6]),
        (CreateTaskField::Notes, sections[8]),
        (CreateTaskField::Files, sections[10]),
    ]
}

fn body_sections(body: Rect, form: &CreateTaskForm) -> std::rc::Rc<[Rect]> {
    let dep_extra = if form.depends_on.is_empty() { 0 } else { 1 };
    let ac_extra = (form.acceptance_criteria.len() as u16).min(5);
    let files_extra = if form.files.is_empty() { 0 } else { 1 };
    let notes_lines: u16 = (form.notes.line_count() as u16 + 2).clamp(4, 8);
    let title_lines: u16 = (form.title.line_count() as u16 + 2).clamp(3, 6);

    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(title_lines),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3 + dep_extra),
            Constraint::Length(1),
            Constraint::Length(3 + ac_extra),
            Constraint::Length(1),
            Constraint::Length(notes_lines),
            Constraint::Length(1),
            Constraint::Length(3 + files_extra),
            Constraint::Min(0),
        ])
        .split(body)
}

pub fn render(app: &App, frame: &mut Frame<'_>) {
    let Some(form) = &app.create_task_form else {
        return;
    };

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    render_hint_bar(form, frame, chunks[0]);
    render_form_body(app, form, frame, chunks[1]);
}

fn render_hint_bar(form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let title = if let Some(id) = &form.editing {
        format!(" Edit Task {}", id.0)
    } else {
        " Create Task".to_string()
    };
    let line = Line::from(vec![
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            "  [Ctrl+S] save · [Tab] next · [Shift+Tab] prev · [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn active_block(title: &str) -> Block<'_> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
}

fn inactive_block(title: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn field_block<'a>(form: &CreateTaskForm, field: CreateTaskField, title: &'a str) -> Block<'a> {
    if form.active_field == field {
        active_block(title)
    } else {
        inactive_block(title)
    }
}

fn render_form_body(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let sections = body_sections(area, form);

    render_title_field(app, form, frame, sections[0]);
    render_epic_priority_assignee_row(app, form, frame, sections[2]);
    render_depends_on_field(app, form, frame, sections[4]);
    render_ac_field(app, form, frame, sections[6]);
    render_notes_field(app, form, frame, sections[8]);
    render_files_field(app, form, frame, sections[10]);
}

fn render_title_field(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let block = field_block(
        form,
        CreateTaskField::Title,
        "Title *  [Shift+Enter] newline",
    );
    let inner = block.inner(area);

    let (cursor_row, _) = form.title.cursor();
    let visible_rows = inner.height as usize;
    let top = if cursor_row + 1 > visible_rows {
        (cursor_row + 1).saturating_sub(visible_rows)
    } else {
        0
    };

    let style = if form.active_field == CreateTaskField::Title {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let lines: Vec<Line> = form
        .title
        .lines()
        .iter()
        .skip(top)
        .take(visible_rows)
        .map(|l| Line::from(l.clone()))
        .collect();

    frame.render_widget(Paragraph::new(lines).style(style).block(block), area);

    if form.active_field == CreateTaskField::Title && cursor_row >= top {
        let visible_row = (cursor_row - top) as u16;
        if visible_row < inner.height {
            let col = form.title.visual_col() as u16;
            let cx = (inner.x + col).min(inner.x + inner.width.saturating_sub(1));
            frame.set_cursor_position(Position::new(cx, inner.y + visible_row));
        }
    }
    let _ = app;
}

fn render_epic_priority_assignee_row(
    app: &App,
    form: &CreateTaskForm,
    frame: &mut Frame<'_>,
    area: Rect,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(area);

    // Epic block
    let epic_block = field_block(form, CreateTaskField::Epic, "Epic");
    let epic_inner = epic_block.inner(cols[0]);
    let epic_line = form.epic.lines().first().cloned().unwrap_or_default();
    let epic_style = if form.active_field == CreateTaskField::Epic {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(Line::from(epic_line))
            .style(epic_style)
            .block(epic_block),
        cols[0],
    );
    if form.active_field == CreateTaskField::Epic {
        let col = form.epic.visual_col() as u16;
        let cx = (epic_inner.x + col).min(epic_inner.x + epic_inner.width.saturating_sub(1));
        frame.set_cursor_position(Position::new(cx, epic_inner.y));
    }

    // Priority selector
    let pri_active = form.active_field == CreateTaskField::Priority;
    let pri_border_style = if pri_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pri_title_style = if pri_active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pri_block = Block::default()
        .title(Span::styled(" Priority ", pri_title_style))
        .borders(Borders::ALL)
        .border_style(pri_border_style);
    let pri_label = format!("◄ {} ►", form.priority);
    let pri_style = if pri_active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(pri_label, pri_style))).block(pri_block),
        cols[1],
    );

    // Assignee selector
    let asgn_active = form.active_field == CreateTaskField::Assignee;
    let asgn_border_style = if asgn_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let asgn_title_style = if asgn_active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let asgn_block = Block::default()
        .title(Span::styled(" Assignee ", asgn_title_style))
        .borders(Borders::ALL)
        .border_style(asgn_border_style);
    let asgn_label = format!("◄ {} ►", form.assignee_label());
    let asgn_style = if asgn_active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(asgn_label, asgn_style))).block(asgn_block),
        cols[2],
    );

    let _ = app;
}

fn render_depends_on_field(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let block = field_block(
        form,
        CreateTaskField::DependsOn,
        "Dependencies  [Enter] add · [Backspace] remove last",
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    if !form.depends_on.is_empty() {
        let chips: Vec<Span> = form
            .depends_on
            .iter()
            .flat_map(|id| {
                [
                    Span::styled(
                        format!("[{}]", id.0),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        lines.push(Line::from(chips));
    }

    let input_text = form.dep_input.lines().first().cloned().unwrap_or_default();
    let input_style = if form.active_field == CreateTaskField::DependsOn {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        if input_text.is_empty() {
            "(type task ID, e.g. T-001)".to_string()
        } else {
            input_text.clone()
        },
        if input_text.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            input_style
        },
    )));

    let available = inner.height as usize;
    let skip = if lines.len() > available {
        lines.len() - available
    } else {
        0
    };
    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();
    frame.render_widget(Paragraph::new(visible_lines), inner);

    if form.active_field == CreateTaskField::DependsOn && !form.dep_input.is_empty() {
        let input_row = (if form.depends_on.is_empty() { 0 } else { 1 }) as u16;
        let col = form.dep_input.visual_col() as u16;
        if input_row < inner.height {
            let cx = (inner.x + col).min(inner.x + inner.width.saturating_sub(1));
            let cy = inner.y + input_row;
            frame.set_cursor_position(Position::new(cx, cy));
        }
    } else if form.active_field == CreateTaskField::DependsOn {
        let input_row = (if form.depends_on.is_empty() { 0 } else { 1 }) as u16;
        if input_row < inner.height {
            frame.set_cursor_position(Position::new(inner.x, inner.y + input_row));
        }
    }

    let _ = app;
}

fn render_ac_field(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let block = field_block(
        form,
        CreateTaskField::AcceptanceCriteria,
        "Acceptance Criteria  [Enter] add · [Backspace] remove last",
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = form
        .acceptance_criteria
        .iter()
        .map(|c| {
            Line::from(vec![
                Span::styled("- ", Style::default().fg(Color::Green)),
                Span::raw(c.clone()),
            ])
        })
        .collect();

    let input_text = form.ac_input.lines().first().cloned().unwrap_or_default();
    let input_style = if form.active_field == CreateTaskField::AcceptanceCriteria {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        if input_text.is_empty() {
            "(type a criterion)".to_string()
        } else {
            input_text.clone()
        },
        if input_text.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            input_style
        },
    )));

    let available = inner.height as usize;
    let skip = if lines.len() > available {
        lines.len() - available
    } else {
        0
    };
    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();
    frame.render_widget(Paragraph::new(visible_lines), inner);

    if form.active_field == CreateTaskField::AcceptanceCriteria {
        let input_row = form.acceptance_criteria.len() as u16;
        if input_row < inner.height {
            let col = form.ac_input.visual_col() as u16;
            let cx = (inner.x + col).min(inner.x + inner.width.saturating_sub(1));
            frame.set_cursor_position(Position::new(cx, inner.y + input_row));
        }
    }

    let _ = app;
}

fn render_notes_field(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let block = field_block(
        form,
        CreateTaskField::Notes,
        "Notes  [Shift+Enter / Alt+Enter] newline",
    );
    let inner = block.inner(area);

    let (cursor_row, _) = form.notes.cursor();
    let visible_rows = inner.height as usize;
    let top = if cursor_row + 1 > visible_rows {
        (cursor_row + 1).saturating_sub(visible_rows)
    } else {
        0
    };

    let note_style = if form.active_field == CreateTaskField::Notes {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let lines: Vec<Line> = form
        .notes
        .lines()
        .iter()
        .skip(top)
        .take(visible_rows)
        .map(|l| Line::from(l.clone()))
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .style(note_style)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );

    if form.active_field == CreateTaskField::Notes && cursor_row >= top {
        let visible_row = (cursor_row - top) as u16;
        if visible_row < inner.height {
            let col = form.notes.visual_col() as u16;
            let cx = (inner.x + col).min(inner.x + inner.width.saturating_sub(1));
            frame.set_cursor_position(Position::new(cx, inner.y + visible_row));
        }
    }

    let _ = app;
}

fn render_files_field(app: &App, form: &CreateTaskForm, frame: &mut Frame<'_>, area: Rect) {
    let block = field_block(
        form,
        CreateTaskField::Files,
        "Files  [Enter] add · [Backspace] remove last",
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    if !form.files.is_empty() {
        let chips: Vec<Span> = form
            .files
            .iter()
            .flat_map(|f| {
                [
                    Span::styled(
                        format!("[{f}]"),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        lines.push(Line::from(chips));
    }

    let input_text = form
        .files_input
        .lines()
        .first()
        .cloned()
        .unwrap_or_default();
    let input_style = if form.active_field == CreateTaskField::Files {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        if input_text.is_empty() {
            "(type a file path)".to_string()
        } else {
            input_text.clone()
        },
        if input_text.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            input_style
        },
    )));

    let available = inner.height as usize;
    let skip = if lines.len() > available {
        lines.len() - available
    } else {
        0
    };
    let visible_lines: Vec<Line> = lines.into_iter().skip(skip).collect();
    frame.render_widget(Paragraph::new(visible_lines), inner);

    if form.active_field == CreateTaskField::Files {
        let input_row = (if form.files.is_empty() { 0 } else { 1 }) as u16;
        if input_row < inner.height {
            let col = form.files_input.visual_col() as u16;
            let cx = (inner.x + col).min(inner.x + inner.width.saturating_sub(1));
            let cy = inner.y + input_row;
            frame.set_cursor_position(Position::new(cx, cy));
        }
    }

    let _ = app;
}
