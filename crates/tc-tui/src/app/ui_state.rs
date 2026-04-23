use std::time::{Duration, Instant};

use crate::keybind::{Direction, PendingChord, WHICH_KEY_DELAY, move_focus};

use super::types::{App, FocusPanel, InputMode};

pub(crate) enum ClipboardPaste {
    Text(String),
    /// Path relative to the store root.
    Image(std::path::PathBuf),
}

impl App {
    pub(super) fn begin_chord(&mut self, chord: PendingChord) {
        self.pending_chord = chord;
        self.chord_started_at = Some(Instant::now());
    }

    pub(super) fn end_chord(&mut self) {
        self.pending_chord = PendingChord::None;
        self.chord_started_at = None;
    }

    pub(super) fn apply_window_move(&mut self, dir: Direction) {
        if let Some(next) = move_focus(self.focus, dir, self.show_log, self.show_dag) {
            self.focus = next;
        }
    }

    pub(super) fn toggle_dag(&mut self) {
        self.show_dag = !self.show_dag;
        if !self.show_dag && self.focus == FocusPanel::Dag {
            self.focus = FocusPanel::Detail;
        }
    }

    pub(super) fn toggle_log(&mut self) {
        self.show_log = !self.show_log;
        if self.show_log {
            self.tail_log();
        } else if self.focus == FocusPanel::Log {
            self.focus = FocusPanel::Tasks;
        }
    }

    /// Returns the chord to show in the which-key popup once the delay has
    /// elapsed. `None` means no popup should be drawn yet (either no chord
    /// is active, or the delay has not expired).
    pub fn which_key_chord(&self) -> Option<PendingChord> {
        if !self.pending_chord.is_active() {
            return None;
        }
        let started = self.chord_started_at?;
        if started.elapsed() >= WHICH_KEY_DELAY {
            Some(self.pending_chord)
        } else {
            None
        }
    }

    /// If a chord is pending but the which-key popup has not yet been shown,
    /// returns how long is left until the popup should appear. Runtime uses
    /// this to shorten the event-poll timeout so the popup does not lag a
    /// full tick.
    pub fn chord_wake_in(&self) -> Option<Duration> {
        let started = self.chord_started_at?;
        let elapsed = started.elapsed();
        if elapsed >= WHICH_KEY_DELAY {
            return None;
        }
        Some(WHICH_KEY_DELAY - elapsed)
    }

    pub(super) fn cycle_focus(&mut self) {
        let cycle: &[FocusPanel] = match (self.show_log, self.show_dag) {
            (false, false) => &[FocusPanel::Epics, FocusPanel::Tasks, FocusPanel::Detail],
            (true, false) => &[
                FocusPanel::Epics,
                FocusPanel::Tasks,
                FocusPanel::Log,
                FocusPanel::Detail,
            ],
            (false, true) => &[
                FocusPanel::Epics,
                FocusPanel::Tasks,
                FocusPanel::Detail,
                FocusPanel::Dag,
            ],
            (true, true) => &[
                FocusPanel::Epics,
                FocusPanel::Tasks,
                FocusPanel::Log,
                FocusPanel::Detail,
                FocusPanel::Dag,
            ],
        };
        let idx = cycle.iter().position(|p| *p == self.focus).unwrap_or(0);
        self.focus = cycle[(idx + 1) % cycle.len()];
    }

    pub(super) fn move_down(&mut self) {
        match self.focus {
            FocusPanel::Epics => {
                if self.selected_epic + 1 < self.epics.len() {
                    self.selected_epic += 1;
                    self.selected_task = 0;
                }
            }
            FocusPanel::Tasks => {
                let len = self.visible_tasks().len();
                if len > 0 && self.selected_task + 1 < len {
                    self.selected_task += 1;
                }
            }
            FocusPanel::Log | FocusPanel::Detail | FocusPanel::Dag => {}
        }
    }

    pub(super) fn move_up(&mut self) {
        match self.focus {
            FocusPanel::Epics => {
                if self.selected_epic > 0 {
                    self.selected_epic -= 1;
                    self.selected_task = 0;
                }
            }
            FocusPanel::Tasks => {
                if self.selected_task > 0 {
                    self.selected_task -= 1;
                }
            }
            FocusPanel::Log | FocusPanel::Detail | FocusPanel::Dag => {}
        }
    }

    pub(super) fn enter_input(&mut self, mode: InputMode, prompt: &str) {
        self.input_mode = mode;
        self.input.clear();
        self.status_message = prompt.to_string();
        if mode == InputMode::AddTask {
            self.restore_draft();
        }
    }

    pub(super) fn draft_path(&self) -> std::path::PathBuf {
        self.store.draft_add_task_path()
    }

    pub(super) fn save_draft(&self) {
        let text = self.input.text();
        if !text.trim().is_empty() {
            let _ = std::fs::write(self.draft_path(), &text);
        }
    }

    pub(super) fn restore_draft(&mut self) {
        let path = self.draft_path();
        if let Ok(text) = std::fs::read_to_string(&path)
            && !text.trim().is_empty()
        {
            self.input.set_text(text.trim_end_matches('\n'));
            self.status_message = "Add task (draft restored): ".to_string();
        }
    }

    pub(super) fn clear_draft(&self) {
        let _ = std::fs::remove_file(self.draft_path());
    }

    /// Exit the current input mode, saving a draft if leaving AddTask with
    /// non-empty content.
    pub(super) fn cancel_input(&mut self) {
        if self.input_mode == InputMode::AddTask && !self.input.is_empty() {
            self.save_draft();
            self.toast("draft saved");
        }
        if self.input_mode == InputMode::LogSearch {
            self.log_view.clear_search();
        }
        self.input_mode = InputMode::Normal;
        self.input.clear();
    }

    /// Read the system clipboard and insert it into the active input. If the
    /// clipboard holds an image, write it to the attachments inbox and
    /// insert a markdown-style reference. Toast on error.
    pub(super) fn paste_clipboard(&mut self) {
        match read_clipboard(self.store.root()) {
            Ok(ClipboardPaste::Text(s)) => self.input.insert_str(&s),
            Ok(ClipboardPaste::Image(rel_path)) => {
                self.input
                    .insert_str(&format!("[Image: {}]", rel_path.display()));
            }
            Err(e) => self.toast(&format!("clipboard: {e}")),
        }
    }
}

/// Read the system clipboard. Prefer image content when present; fall back to
/// text. When an image is returned, it is also written to
/// `{root}/.tc/attachments/inbox/{unix_ts}.png`.
pub(crate) fn read_clipboard(root: &std::path::Path) -> Result<ClipboardPaste, String> {
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    if let Ok(image) = clip.get_image() {
        let rel = write_image_attachment(root, &image)?;
        return Ok(ClipboardPaste::Image(rel));
    }
    let text = clip.get_text().map_err(|e| e.to_string())?;
    Ok(ClipboardPaste::Text(text))
}

/// Encode `image` as a PNG under `.tc/attachments/inbox/{ts}.png`. Returns
/// the path relative to `root` (so the reference inserted into the task
/// survives a checkout from another machine).
pub(crate) fn write_image_attachment(
    root: &std::path::Path,
    image: &arboard::ImageData<'_>,
) -> Result<std::path::PathBuf, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rel = std::path::PathBuf::from(".tc/attachments/inbox").join(format!("{ts}.png"));
    let abs = root.join(&rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let png = encode_png(image)?;
    std::fs::write(&abs, png).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Encode RGBA pixel data as PNG using the encoder re-exported by `arboard`.
fn encode_png(image: &arboard::ImageData<'_>) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(image.bytes.as_ref())
            .map_err(|e| e.to_string())?;
    }
    Ok(buf)
}
