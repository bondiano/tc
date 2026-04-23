use crossterm::event::KeyCode;
use tc_core::config::{ExecutionMode, ExecutorKind, SandboxPolicy, TcConfig};
use tc_executor::any::is_installed;

use crate::error::{TuiError, TuiResult};

use super::types::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Executor,
    Mode,
    Sandbox,
}

impl SettingsField {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Executor => Self::Mode,
            Self::Mode => Self::Sandbox,
            Self::Sandbox => Self::Executor,
        }
    }

    pub(super) fn prev(self) -> Self {
        match self {
            Self::Executor => Self::Sandbox,
            Self::Mode => Self::Executor,
            Self::Sandbox => Self::Mode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub field: SettingsField,
    pub executor: ExecutorKind,
    pub mode: ExecutionMode,
    pub sandbox: SandboxPolicy,
    pub dirty: bool,
}

impl SettingsState {
    pub fn from_config(cfg: &TcConfig) -> Self {
        let current = cfg.executor.default;
        let executor = if is_installed(current, cfg) {
            current
        } else {
            first_installed(cfg).unwrap_or(current)
        };
        Self {
            field: SettingsField::Executor,
            executor,
            mode: cfg.executor.mode,
            sandbox: cfg.executor.sandbox.enabled,
            dirty: false,
        }
    }
}

/// First executor from `ExecutorKind::ALL` whose backing binary is on PATH.
fn first_installed(cfg: &TcConfig) -> Option<ExecutorKind> {
    'scan: for kind in ExecutorKind::ALL {
        if is_installed(*kind, cfg) {
            return Some(*kind);
        }
        continue 'scan;
    }
    None
}

impl App {
    pub(super) fn open_settings(&mut self) {
        self.settings = Some(SettingsState::from_config(&self.config));
    }

    pub(super) fn close_settings(&mut self) {
        self.settings = None;
    }

    pub(super) fn save_settings(&mut self) -> TuiResult<()> {
        let Some(state) = self.settings.as_ref() else {
            return Ok(());
        };
        let mut new_cfg = self.config.clone();
        new_cfg.executor.default = state.executor;
        new_cfg.executor.mode = state.mode;
        new_cfg.executor.sandbox.enabled = state.sandbox;
        new_cfg
            .validate()
            .map_err(|e| TuiError::Render(format!("invalid settings: {e}")))?;
        self.store.save_config(&new_cfg)?;
        self.config = new_cfg;
        self.settings = None;
        self.toast("settings saved");
        Ok(())
    }

    pub(super) fn on_key_settings(&mut self, code: KeyCode) -> TuiResult<()> {
        let Some(state) = self.settings.as_mut() else {
            return Ok(());
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close_settings();
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                state.field = state.field.next();
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => {
                state.field = state.field.prev();
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => {
                cycle_settings_field(state, 1, &self.config);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                cycle_settings_field(state, -1, &self.config);
            }
            KeyCode::Enter => {
                if state.dirty {
                    self.save_settings()?;
                } else {
                    self.close_settings();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn cycle_settings_field(state: &mut SettingsState, delta: i32, cfg: &TcConfig) {
    match state.field {
        SettingsField::Executor => {
            if let Some(next) = cycle_executor(state.executor, delta, cfg)
                && next != state.executor
            {
                state.executor = next;
                state.dirty = true;
            }
        }
        SettingsField::Mode => {
            let next = cycle_variant(ExecutionMode::ALL, state.mode, delta);
            if next != state.mode {
                state.mode = next;
                state.dirty = true;
            }
        }
        SettingsField::Sandbox => {
            let next = cycle_variant(SandboxPolicy::ALL, state.sandbox, delta);
            if next != state.sandbox {
                state.sandbox = next;
                state.dirty = true;
            }
        }
    }
}

/// Advance by `delta` through `options`, wrapping. Assumes `options` is
/// non-empty and `current` appears in it (falls back to index 0 otherwise).
fn cycle_variant<T: Copy + PartialEq>(options: &[T], current: T, delta: i32) -> T {
    let len = options.len() as i32;
    let start = options.iter().position(|o| *o == current).unwrap_or(0) as i32;
    let idx = (start + delta).rem_euclid(len) as usize;
    options[idx]
}

/// Cycle executors, preferring kinds whose binary is on PATH.
///
/// If at least one other executor is installed, skip uninstalled ones so
/// the user can't pick something that will fail to launch. If *nothing*
/// is installed (e.g. the detection is wrong, or we're in a test), fall
/// back to a plain cycle so the UI stays usable.
fn cycle_executor(current: ExecutorKind, delta: i32, cfg: &TcConfig) -> Option<ExecutorKind> {
    let opts = ExecutorKind::ALL;
    let any_installed = opts.iter().any(|k| is_installed(*k, cfg));
    let len = opts.len() as i32;
    let start = opts.iter().position(|k| *k == current).unwrap_or(0) as i32;
    let mut idx = start;
    'scan: for _ in 0..opts.len() {
        idx = (idx + delta).rem_euclid(len);
        let candidate = opts[idx as usize];
        if !any_installed || is_installed(candidate, cfg) {
            return Some(candidate);
        }
        continue 'scan;
    }
    None
}
