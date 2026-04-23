use crossterm::event::KeyCode;
use tc_core::config::TcConfig;

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
    pub executor: String,
    pub mode: String,
    pub sandbox: String,
    pub dirty: bool,
}

impl SettingsState {
    pub fn from_config(cfg: &TcConfig) -> Self {
        Self {
            field: SettingsField::Executor,
            executor: cfg.executor.default.clone(),
            mode: cfg.executor.mode.clone(),
            sandbox: cfg.executor.sandbox.enabled.clone(),
            dirty: false,
        }
    }
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
        new_cfg.executor.default = state.executor.clone();
        new_cfg.executor.mode = state.mode.clone();
        new_cfg.executor.sandbox.enabled = state.sandbox.clone();
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
                cycle_settings_field(state, 1);
            }
            KeyCode::Char('h') | KeyCode::Left => {
                cycle_settings_field(state, -1);
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

pub(super) fn cycle_settings_field(state: &mut SettingsState, delta: i32) {
    use crate::components::settings::{EXECUTOR_OPTIONS, MODE_OPTIONS, SANDBOX_OPTIONS};

    let (options, current) = match state.field {
        SettingsField::Executor => (EXECUTOR_OPTIONS, &mut state.executor),
        SettingsField::Mode => (MODE_OPTIONS, &mut state.mode),
        SettingsField::Sandbox => (SANDBOX_OPTIONS, &mut state.sandbox),
    };
    if options.is_empty() {
        return;
    }
    let idx = options
        .iter()
        .position(|o| *o == current.as_str())
        .unwrap_or(0) as i32;
    let len = options.len() as i32;
    let next = ((idx + delta).rem_euclid(len)) as usize;
    let next_val = options[next].to_string();
    if *current != next_val {
        *current = next_val;
        state.dirty = true;
    }
}
