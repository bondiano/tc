use std::time::Duration;

use crate::app::FocusPanel;

pub const WHICH_KEY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingChord {
    None,
    CtrlW,
    Leader,
    LeaderWindow,
    LeaderTask,
    LeaderView,
}

impl PendingChord {
    pub fn is_active(self) -> bool {
        !matches!(self, PendingChord::None)
    }
}

/// Compute the next focus after moving in `dir` from `current`, given which
/// optional panels are visible. Returns `None` when the move has no target.
///
/// Layout grid (col, row):
///   Epics:  (0, 0)
///   Tasks:  (1, 0)     Log:    (1, 1) (when show_log)
///   Detail: (2, 0)     DAG:    (2, 1) (when show_dag)
///
/// Horizontal moves prefer to keep the same row; if the destination column
/// has no panel in that row, they fall back to row 0. Vertical moves never
/// change column and do not fall back.
pub fn move_focus(
    current: FocusPanel,
    dir: Direction,
    show_log: bool,
    show_dag: bool,
) -> Option<FocusPanel> {
    let (col, row) = pos(current);
    match dir {
        Direction::Left => {
            if col == 0 {
                return None;
            }
            let target_col = col - 1;
            panel_at(target_col, row, show_log, show_dag)
                .or_else(|| panel_at(target_col, 0, show_log, show_dag))
        }
        Direction::Right => {
            if col == 2 {
                return None;
            }
            let target_col = col + 1;
            panel_at(target_col, row, show_log, show_dag)
                .or_else(|| panel_at(target_col, 0, show_log, show_dag))
        }
        Direction::Up => {
            if row == 0 {
                return None;
            }
            panel_at(col, row - 1, show_log, show_dag)
        }
        Direction::Down => {
            if row == 1 {
                return None;
            }
            panel_at(col, row + 1, show_log, show_dag)
        }
    }
}

fn pos(p: FocusPanel) -> (u8, u8) {
    match p {
        FocusPanel::Epics => (0, 0),
        FocusPanel::Tasks => (1, 0),
        FocusPanel::Log => (1, 1),
        FocusPanel::Detail => (2, 0),
        FocusPanel::Dag => (2, 1),
    }
}

fn panel_at(col: u8, row: u8, show_log: bool, show_dag: bool) -> Option<FocusPanel> {
    match (col, row) {
        (0, 0) => Some(FocusPanel::Epics),
        (1, 0) => Some(FocusPanel::Tasks),
        (1, 1) if show_log => Some(FocusPanel::Log),
        (2, 0) => Some(FocusPanel::Detail),
        (2, 1) if show_dag => Some(FocusPanel::Dag),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_left_goes_to_epics() {
        assert_eq!(
            move_focus(FocusPanel::Tasks, Direction::Left, false, false),
            Some(FocusPanel::Epics)
        );
    }

    #[test]
    fn tasks_right_goes_to_detail() {
        assert_eq!(
            move_focus(FocusPanel::Tasks, Direction::Right, false, false),
            Some(FocusPanel::Detail)
        );
    }

    #[test]
    fn epics_left_is_none() {
        assert_eq!(
            move_focus(FocusPanel::Epics, Direction::Left, false, false),
            None
        );
    }

    #[test]
    fn detail_right_is_none() {
        assert_eq!(
            move_focus(FocusPanel::Detail, Direction::Right, false, false),
            None
        );
    }

    #[test]
    fn tasks_down_requires_log_visible() {
        assert_eq!(
            move_focus(FocusPanel::Tasks, Direction::Down, false, false),
            None
        );
        assert_eq!(
            move_focus(FocusPanel::Tasks, Direction::Down, true, false),
            Some(FocusPanel::Log)
        );
    }

    #[test]
    fn detail_down_requires_dag_visible() {
        assert_eq!(
            move_focus(FocusPanel::Detail, Direction::Down, false, false),
            None
        );
        assert_eq!(
            move_focus(FocusPanel::Detail, Direction::Down, false, true),
            Some(FocusPanel::Dag)
        );
    }

    #[test]
    fn log_left_falls_back_to_epics() {
        assert_eq!(
            move_focus(FocusPanel::Log, Direction::Left, true, false),
            Some(FocusPanel::Epics)
        );
    }

    #[test]
    fn log_right_prefers_dag_when_visible() {
        assert_eq!(
            move_focus(FocusPanel::Log, Direction::Right, true, true),
            Some(FocusPanel::Dag)
        );
        assert_eq!(
            move_focus(FocusPanel::Log, Direction::Right, true, false),
            Some(FocusPanel::Detail)
        );
    }

    #[test]
    fn log_up_goes_to_tasks() {
        assert_eq!(
            move_focus(FocusPanel::Log, Direction::Up, true, false),
            Some(FocusPanel::Tasks)
        );
    }

    #[test]
    fn dag_up_goes_to_detail() {
        assert_eq!(
            move_focus(FocusPanel::Dag, Direction::Up, false, true),
            Some(FocusPanel::Detail)
        );
    }

    #[test]
    fn dag_left_prefers_log_when_visible() {
        assert_eq!(
            move_focus(FocusPanel::Dag, Direction::Left, true, true),
            Some(FocusPanel::Log)
        );
        assert_eq!(
            move_focus(FocusPanel::Dag, Direction::Left, false, true),
            Some(FocusPanel::Tasks)
        );
    }
}
